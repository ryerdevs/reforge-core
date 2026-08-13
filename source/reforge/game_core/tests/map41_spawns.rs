//! Integration F5 — extracción + EXPANSIÓN de spawns del mapa 41 contra el
//! RUNTIME REAL (WSL) — gated con `#[ignore]`. Evidencia de la TAREA 2 del
//! lane F5: el parseo replicado + la expansión de grupos producen la MISMA
//! fauna que el core C++ (`SECTREE_MANAGER::Build` + `regen_load` +
//! `SpawnGroup`/`SpawnGroupGroup`, sectree_manager.cpp:654-741 +
//! char_manager.cpp:545-613).
//!
//! Rutas del runtime (verificadas 2026-08-11):
//! - WSL: `/home/m2/source/metin2_svfiles/main/srv1/share/locale/spain/map`
//!   (las tablas de grupos viven un nivel arriba: `.../locale/spain/`)
//! - Windows (con WSL activo): `\\wsl$\Debian-M2\home\m2\source\...`
//!
//! Ejecutar (WSL activo):
//!
//! ```text
//! cd C:\projects\Metin2\source\reforge
//! cargo test --package game_core -- --ignored map41
//! ```
//!
//! SOLO lectura del runtime (regla del lane F5 — WSL read-only).

use game_core::npc::{load_map_spawns, SpawnKind};
use std::collections::HashMap;

/// La ruta del runtime: se prueba la vista UNC de Windows primero y la
/// absoluta de WSL después (el test puede correr en cualquiera de los dos).
fn runtime_map_paths() -> Vec<String> {
    vec![
        r"\\wsl$\Debian-M2\home\m2\source\metin2_svfiles\main\srv1\share\locale\spain\map"
            .to_string(),
        "/home/m2/source/metin2_svfiles/main/srv1/share/locale/spain/map".to_string(),
    ]
}

fn load_map41() -> (String, Vec<game_core::npc::SpawnEntry>) {
    let mut last_err = String::new();
    for path in runtime_map_paths() {
        match load_map_spawns(41, &path) {
            Ok(entries) => return (path, entries),
            Err(e) => last_err = format!("{path}: {e}"),
        }
    }
    panic!("runtime del mapa 41 inaccesible: {last_err}");
}

/// El mapa 41 EXPANDIDO del runtime: 23.033 mobs individuales (Σ count) de
/// 117 vnums distintos — la fauna completa (regen 938 entradas `r` ->
/// miembros de TODOS los grupos de cada gg; npc/boss/stone directos + sus
/// grupos). Perro Salvaje (101) y Lobo (102) PRESENTES (el GAP reportado:
/// los mobs de bajo nivel no spawneaban).
#[test]
#[ignore = "requiere el runtime WSL (share/locale/spain/map)"]
fn map41_spawns_against_real_runtime() {
    let (path, entries) = load_map41();
    eprintln!("runtime: {path}");
    eprintln!("entries (vnum+pos): {}", entries.len());
    let total: u64 = entries.iter().map(|e| e.count as u64).sum();
    eprintln!("total mobs (Σ count): {total}");
    assert_eq!(total, 23_033, "expansión completa (verificado 2026-08-11)");

    // El resultado NO contiene kinds de grupo (la colisión de vnums quedó
    // resuelta: los vnums de grupo no se emiten — doc módulo).
    assert!(
        entries
            .iter()
            .all(|e| matches!(e.kind, SpawnKind::Mob | SpawnKind::Anywhere)),
        "solo Mob/Anywhere tras la expansión"
    );

    // Inventario por vnum (Σ count) — spot de los mobs de bajo nivel.
    let mut by_vnum: HashMap<u32, u64> = HashMap::new();
    for e in &entries {
        *by_vnum.entry(e.vnum).or_default() += e.count as u64;
    }
    eprintln!("distinct vnums: {}", by_vnum.len());
    assert_eq!(by_vnum.len(), 117, "vnums distintos");
    // Perro Salvaje (lvl 1, 126 HP) — el GAP que motivó la expansión.
    assert_eq!(by_vnum.get(&101), Some(&840), "Perro Salvaje 101");
    assert_eq!(by_vnum.get(&102), Some(&882), "Lobo 102");
    assert_eq!(by_vnum.get(&2101), Some(&2492), "Zorro del Desierto 2101");
    // Mobs del vecindario de nivel bajo-medio.
    assert_eq!(by_vnum.get(&171), Some(&546), "Perro Callejero Hambriento");
    assert_eq!(by_vnum.get(&104), Some(&745), "Lobo Azul");
    // NPCs/mobs directos sin expansión (npc.txt / boss.txt / stone.txt).
    assert_eq!(by_vnum.get(&20340), Some(&1), "Maestro Fuerza Corporal");
    assert_eq!(by_vnum.get(&9001), Some(&1), "Vendedor Tienda de Armas");
    assert_eq!(by_vnum.get(&151), Some(&3), "Cung-Mok (boss.txt m ×3)");
    assert_eq!(by_vnum.get(&8001), Some(&2), "Metin de Dolor (stone ×2)");
    assert_eq!(by_vnum.get(&5001), Some(&1), "Pirata Tanaka (Anywhere)");
    assert_eq!(by_vnum.get(&5004), Some(&10), "Pirata Tanaka ×10 (Anywhere)");
    // El grupo 12007 del npc.txt (Pony/Caballo + líder "." 20025).
    assert_eq!(by_vnum.get(&20025), Some(&1), "líder '.' del grupo 12007");
    assert_eq!(by_vnum.get(&20029), Some(&2), "Pony ×2");
    assert_eq!(by_vnum.get(&20030), Some(&2), "Caballo ×2");
    // Vetas (stone.txt `r` 2001 -> 25 copias × 5 grupos de veta).
    assert_eq!(by_vnum.get(&20049), Some(&25), "Veta de Madera Fósil");
    assert_eq!(by_vnum.get(&20050), Some(&25), "Veta de Cobre");
    // Bosses de grupo (boss.txt `g` 139/140/141 + 317/318).
    assert_eq!(by_vnum.get(&191), Some(&2), "Lykos (grupo 139 + directo)");
    assert_eq!(by_vnum.get(&192), Some(&1), "Scrofa (grupo 140)");
    assert_eq!(by_vnum.get(&193), Some(&1), "Bera (grupo 141)");
    assert_eq!(by_vnum.get(&394), Some(&2), "Jin-Hee (grupo 318)");
    // Warp del npc.txt (10005) — directo (parity: el C++ lo spawnea).
    assert_eq!(by_vnum.get(&10005), Some(&1), "warp Area_Bakra");

    // Spots de parity contra el core: el NPC 20340 del npc.txt en el centro
    // del punto (444, 623) + base (921600, 204800) = (966000, 267100).
    let npc = entries.iter().find(|e| e.vnum == 20340).expect("20340");
    assert_eq!((npc.x, npc.y), (966000, 267100), "20340 en UNITS");
    // Los miembros del group-group 101 (a1_01) en el centro del regen
    // (360, 425) = (957600, 247300) — Perro Salvaje 101 está AHÍ.
    let dog = entries.iter().find(|e| e.vnum == 101).expect("101 expandido");
    assert_eq!((dog.x, dog.y), (957600, 247300), "manada a1_01 en UNITS");
    assert_eq!(dog.count, 3, "3 Perro Salvaje por manada (líder + 2)");
    // Anywhere 5004 anclado en el Town spawn del mapa 41 (480, 736).
    let any = entries
        .iter()
        .find(|e| e.vnum == 5004 && e.kind == SpawnKind::Anywhere)
        .expect("5004 Anywhere");
    assert_eq!((any.x, any.y), (969600, 278400), "Town spawn (parity LoadMapRegion)");
}

/// Inventario expandido — TODAS las entradas son mobs directos (la colisión
/// de vnums desapareció: 117 vnums distintos). Los vnums de GRUPO puros no
/// se emiten: 317/318/139/140/141/12007 (boss.txt/npc.txt `g`) y 2001
/// (stone.txt `r`) NO aparecen como spawns — solo sus miembros.
#[test]
#[ignore = "requiere el runtime WSL (share/locale/spain/map)"]
fn map41_direct_mob_vnums_inventory() {
    let (_path, entries) = load_map41();
    let mut vnums: Vec<u32> = entries.iter().map(|e| e.vnum).collect();
    vnums.sort_unstable();
    vnums.dedup();
    eprintln!("direct mob vnums ({}): {vnums:?}", vnums.len());
    assert_eq!(vnums.len(), 117);
    for v in &vnums {
        assert!(*v > 0, "vnum positivo: {v}");
    }
    // Vnums de grupo puro (no son mobs): nunca se emiten tras la expansión.
    for gv in [317u32, 318, 139, 140, 141, 12007, 2001] {
        assert!(!vnums.contains(&gv), "vnum de grupo {gv} no debe emitirse");
    }
    // Perro Salvaje 101 PRESENTE como mob (el GAP reportado).
    assert!(vnums.contains(&101), "Perro Salvaje en el inventario");
}
