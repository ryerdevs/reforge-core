//! Dominio SKILL del mundo (C5): el sistema 4 del tick (`affects_system` —
//! buffs server-timed) + los métodos del `WorldSim` de skills (`load_skills`,
//! `process_skill`). Las fórmulas puras viven en `game_core::skill`.

use bevy_ecs::prelude::*;

use crate::combat::{
    attack_power, attack_speed_for_weapon, calc_attack_rating, def_grade_npc, distance_approx,
    player_def_grade, PlayerState,
};
use crate::ecs::components::{Affect, Affects, Hp, Mp, Player, Position, SkillCooldowns, SkillLevels};
use crate::ecs::events::{KillInfo, NpcEvent, SkillEvent, SplashVictimInfo};
use crate::ecs::resources::{NpcIndex, NpcOutbox, Rand, SkillPowerTable, SkillTable, Tick};
use crate::ecs::world::WorldSim;
use crate::skill::{
    attr_type, damage_flag_for_attr, eval_poly, k_value, skill_damage, skill_level_from_blob,
    point, SkillProto, SkillRepo, skill_flag,
};
use database::item::ProtoItem;

/// 4) BUFFS server-timed (parity `ProcessAffect` — char_affect.cpp:204-260):
///    cada tick decrementa la duración de los afectos del jugador; al
///    expirar, revierte el efecto aplicado (MAX_HP/MAX_SP — el máximo baja y
///    el actual se clampa) y emite `AffectRemoved` (GC_AFFECT_REMOVE en el
///    canal). El legacy corre una vez por segundo; el tick de 500 ms da
///    expiración fina (documentado).
pub(crate) fn affects_system(
    mut players: Query<(&Player, &mut Affects, &mut Hp, &mut Mp)>,
    tick: Res<Tick>,
    mut outbox: ResMut<NpcOutbox>,
) {
    for (p, mut affects, mut hp, mut mp) in &mut players {
        let dt = tick.dt_ms;
        let mut i = 0;
        while i < affects.0.len() {
            if affects.0[i].duration_ms <= dt {
                let aff = affects.0.remove(i);
                // Revertir el efecto aplicado (parity `ComputeAffect(bAdd=false)`
                // — PointChange(MAX_HP/MAX_SP, -value); el actual se clampa).
                match aff.point {
                    point::MAX_HP => {
                        hp.max_hp = (hp.max_hp - aff.value).max(0);
                        hp.hp = hp.hp.min(hp.max_hp);
                    }
                    point::MAX_SP => {
                        mp.max_mp = (mp.max_mp - aff.value).max(0);
                        mp.mp = mp.mp.min(mp.max_mp);
                    }
                    _ => {} // DEF/ATT/MOV/etc: se calculan en vivo desde Affects
                }
                outbox.0.push(SkillEvent::AffectRemoved {
                    player_vid: p.vid,
                    skill_id: aff.skill_id,
                    point: aff.point,
                }.into());
            } else {
                affects.0[i].duration_ms -= dt;
                i += 1;
            }
        }
    }
}

impl WorldSim {
    /// Carga (una vez) la tabla de skills del `player.skill_proto` (PG — el
    /// canal la llama tras el join del primer jugador). Errores → `Err` (el
    /// canal degrada a skills desactivadas).
    pub async fn load_skills(&mut self, repo: &SkillRepo) -> Result<(), String> {
        if !self.world.resource::<SkillTable>().0.is_empty() {
            return Ok(());
        }
        let protos = repo.load_all().await?;
        let map = protos.into_iter().map(|p| (p.vnum, p)).collect();
        self.world.resource_mut::<SkillTable>().0 = map;
        Ok(())
    }

    /// Resuelve el CG_USE_SKILL EN EL MUNDO (server-authoritative — parity
    /// `CHARACTER::UseSkill` + `ComputeSkill` + `FuncSplashDamage::OnHit`).
    ///
    /// Validaciones en orden (rechazos SILENCIOSOS — parity: el legacy
    /// devuelve false sin respuesta; el cliente muestra su propio cooldown):
    /// 1. nivel del jugador en la skill (GetSkillLevel == 0 → false);
    /// 2. objetivo (mob para daño; jugador/self para buffs) + rango
    ///    (`dwtargetrange > 0` → `dist <= range + 50` — parity ComputeSkill);
    /// 3. cooldown del skill (`TSkillUseInfo::dwNextSkillUsableTime`);
    /// 4. coste SP (o HP con USE_HP_AS_COST) — `GetSP() < needed → false`.
    ///    DESVIACIÓN defensiva documentada: el legacy gasta SP ANTES del
    ///    gate de cooldown/rango (char_skill.cpp:2509-2545) — el Rust valida
    ///    primero para no penalizar un click bloqueado (ADR-0011).
    ///
    /// Efecto: daño del skill (poly → `skill_damage` → `AttackResult`-shape
    /// en `SkillResult`) o buff (`Affects` + `GC_AFFECT_ADD`).
    pub(crate) fn process_skill(
        &mut self,
        player_vid: u32,
        skill_id: u32,
        target_vid: u32,
        weapon: Option<&ProtoItem>,
        now_ms: u64,
    ) -> Vec<NpcEvent> {
        let Some(pe) = self.players.get(&player_vid).copied() else {
            return Vec::new();
        };
        let Some(proto) = self.world.resource::<SkillTable>().0.get(&skill_id).cloned() else {
            return Vec::new(); // skill desconocida (o tabla sin cargar)
        };
        let (px, py, level, job, skill_group, st, dx, iq, ht, armor, hp, mp, max_hp, max_mp, skill_blob) = {
            let Ok(ent) = self.world.get_entity(pe) else { return Vec::new() };
            let Some(pos) = ent.get::<Position>() else { return Vec::new() };
            let Some(p) = ent.get::<Player>() else { return Vec::new() };
            let Some(h) = ent.get::<Hp>() else { return Vec::new() };
            let Some(m) = ent.get::<Mp>() else { return Vec::new() };
            let Some(sk) = ent.get::<SkillLevels>() else { return Vec::new() };
            (
                pos.x, pos.y, p.level, p.job, p.skill_group, p.st, p.dx, p.iq, p.ht, p.armor,
                h.hp, m.mp, h.max_hp, m.max_mp, sk.0.clone(),
            )
        };
        // (1) nivel del jugador en la skill (parity GetSkillLevel == 0).
        let sk_level = skill_level_from_blob(&skill_blob, skill_id);
        if sk_level == 0 {
            return Vec::new();
        }
        // `k` del poly, parity char_skill.cpp:1632 (`k = GetSkillPower(vnum,
        // level) * bMaxLevel / 100`): el poder REAL por job/skillgroup/nivel
        // de la tabla `common.locale` (skill_power). Fail-open: sin tabla
        // cargada → la aproximación `level × max_level / 100` (desviación
        // documentada, F6 balance) — el server nunca rompe por esto.
        let k = {
            let table = self.world.resource::<SkillPowerTable>().0.clone();
            if table.loaded() {
                let power = table.skill_power(job, skill_group, i32::from(sk_level), false);
                f64::from(power) * f64::from(proto.max_level) / 100.0
            } else {
                k_value(sk_level, proto.max_level)
            }
        };

        // (2) objetivo + rango. Daño single-target → el mob del NpcIndex;
        // SPLASH (área) → el CENTRO = la posición del target del wire (mob
        // o PC del mundo) o la del caster sin target válido (parity
        // UseSkill → ComputeSkill: `FuncSplashDamage f(pkVictim->GetX(),
        // pkVictim->GetY(), ...)` — el target define el centro); buff →
        // self (SELFONLY o target propio) u otro jugador.
        let mut victim_mob: Option<(i32, i32, i32, i32)> = None; // (dx, lv, def, max_hp)
        let mut buff_target = pe;
        let mut splash_center: Option<(i32, i32)> = None;
        if proto.is_attack() {
            if proto.flag & skill_flag::SPLASH != 0 {
                // Centro del área (parity ComputeSkillAtPosition posTarget).
                let center = if let Some(v) = self.npc_view(target_vid) {
                    (v.state.x, v.state.y)
                } else if let Some(&e) = self.players.get(&target_vid)
                    && let Ok(ent) = self.world.get_entity(e)
                    && let Some(pos) = ent.get::<Position>()
                {
                    (pos.x, pos.y)
                } else {
                    (px, py) // sin target válido → el caster
                };
                // Rango al CENTRO (parity ComputeSkill — el check del
                // dwTargetRange contra el objetivo).
                let dist = distance_approx(px - center.0, py - center.1);
                if proto.target_range > 0 && dist > proto.target_range as i32 + 50 {
                    return Vec::new();
                }
                splash_center = Some(center);
            } else {
                let Some(view) = self.npc_view(target_vid) else { return Vec::new() };
                let dist = distance_approx(px - view.state.x, py - view.state.y);
                if proto.target_range > 0 && dist > proto.target_range as i32 + 50 {
                    return Vec::new();
                }
                victim_mob = Some((
                    view.state.dx,
                    view.state.level,
                    def_grade_npc(view.state.level, view.state.ht, view.state.wdef),
                    view.max_hp,
                ));
            }
        } else {
            if proto.flag & skill_flag::SELFONLY != 0 || target_vid == player_vid {
                buff_target = pe;
            } else if let Some(&e) = self.players.get(&target_vid) {
                buff_target = e; // buff a otro jugador (subset — sin PARTY)
            } else {
                return Vec::new(); // objetivo no es un jugador (mob) — fuera
            }
            // Rango al objetivo del buff (parity ComputeSkill).
            let dist = {
                let Ok(ent) = self.world.get_entity(buff_target) else { return Vec::new() };
                let Some(pos) = ent.get::<Position>() else { return Vec::new() };
                distance_approx(px - pos.x, py - pos.y)
            };
            if proto.target_range > 0 && dist > proto.target_range as i32 + 50 {
                return Vec::new();
            }
        }

        // (3) cooldown del skill (parity TSkillUseInfo — el rechazo NO gasta
        // SP; desviación documentada arriba).
        {
            let Ok(mut ent) = self.world.get_entity_mut(pe) else { return Vec::new() };
            let Some(cds) = ent.get_mut::<SkillCooldowns>() else { return Vec::new() };
            if cds.0.get(&skill_id).is_some_and(|t| *t > now_ms) {
                return Vec::new();
            }
        }

        // Evalúa los polys (el RNG del mundo — `number()` del poly).
        let (sp_cost, hp_cost, cooldown_ms, amount, duration_secs) = {
            let mut rng = self.world.resource_mut::<Rand>();
            let mut roll = |lo: i32, hi: i32| rng.roll(lo, hi);
            // El `atk` del poly: CalcMeleeDamage sin la DEF del objetivo
            // (parity battle.cpp:74-183) — las vars del ataque del jugador.
            let player_state = PlayerState {
                vid: player_vid,
                x: px,
                y: py,
                level,
                ht,
                job,
                st,
                dx,
                iq,
                attack_speed_ms: attack_speed_for_weapon(weapon),
                att_grade_bonus: 0,
                critical_pct: 0,
            };
            let (victim_dx, victim_lv) = match victim_mob {
                Some((vdx, vlv, _, _)) => (vdx, vlv),
                None => (dx, level), // buff a self: el ar es contra uno mismo
            };
            let atk = attack_power(&player_state, victim_dx, victim_lv, weapon, &mut roll) as f64;
            let ar = calc_attack_rating(dx, level, victim_dx, victim_lv) as f64;
            let def = f64::from(player_def_grade(level, ht, armor));
            let odef = def; // sin DEF_GRADE_BONUS en el subset (buffs aparte)
            let maxhp = match victim_mob { Some((_, _, _, mhp)) => f64::from(mhp), None => f64::from(max_hp) };
            let maxsp = if victim_mob.is_some() { 0.0 } else { f64::from(max_mp) };
            let var = |name: &str| -> Option<f64> {
                match name {
                    "k" => Some(k),
                    "atk" => Some(atk),
                    "lv" => Some(f64::from(level)),
                    "iq" => Some(f64::from(iq)),
                    "str" => Some(f64::from(st)),
                    "dex" => Some(f64::from(dx)),
                    "con" => Some(f64::from(ht)),
                    "maxhp" => Some(maxhp),
                    "maxsp" => Some(maxsp),
                    "ar" => Some(ar),
                    "def" => Some(def),
                    "odef" => Some(odef),
                    // coste SP: v = SP actual (o HP con USE_HP_AS_COST).
                    "v" => Some(f64::from(if proto.flag & skill_flag::USE_HP_AS_COST != 0 { hp } else { mp })),
                    "maxv" => Some(f64::from(max_mp)),
                    _ => None,
                }
            };
            let mut eval = |expr: &str| eval_poly(expr, &var, &mut roll);
            let sp_cost = eval(&proto.sp_cost_poly).unwrap_or(0.0) as i32;
            let hp_cost = if proto.flag & skill_flag::USE_HP_AS_COST != 0 {
                sp_cost
            } else {
                0
            };
            let cooldown_ms = (eval(&proto.cooldown_poly).unwrap_or(0.0) * 1000.0) as u64;
            let amount = eval(&proto.point_poly).unwrap_or(0.0) as i32;
            let duration_secs = eval(&proto.duration_poly).unwrap_or(0.0) as i32;
            (sp_cost, hp_cost, cooldown_ms, amount, duration_secs)
        };

        // (4) coste SP/HP (parity GetSP() < needed → false).
        if sp_cost > 0 && mp < sp_cost {
            return Vec::new();
        }
        if hp_cost > 0 && hp < hp_cost {
            return Vec::new();
        }

        // Aplicar el coste + el cooldown (borrows secuenciales).
        if let Ok(mut ent) = self.world.get_entity_mut(pe) {
            if let Some(mut m) = ent.get_mut::<Mp>() {
                m.mp = (m.mp - sp_cost).max(0);
            }
            if let Some(mut h) = ent.get_mut::<Hp>() {
                h.hp = (h.hp - hp_cost).max(0);
            }
            if let Some(mut cds) = ent.get_mut::<SkillCooldowns>() {
                cds.0.insert(skill_id, now_ms + cooldown_ms);
            }
        }

        let mut events = Vec::new();
        if proto.is_attack() {
            if let Some(center) = splash_center {
                // SPLASH (área): el daño a CADA víctima dentro del radio
                // (el coste SP/cooldown ya se pagó UNA vez arriba).
                events = self.splash_damage(
                    player_vid,
                    pe,
                    skill_id,
                    &proto,
                    center,
                    target_vid,
                    weapon,
                    sp_cost,
                    hp_cost,
                    k,
                    level,
                    job,
                    st,
                    dx,
                    iq,
                    ht,
                    armor,
                );
            } else {
            // Daño del skill (parity FuncSplashDamage::OnHit): la DB da el
            // poly en NEGATIVO → `iAmount = -iAmount` → CalcBattleDamage
            // floor → ajuste por attr (MELEE: -victim DEF).
            let (_, _, victim_def, _) = victim_mob.expect("mob del ataque");
            let amount = -amount; // iAmount = -iAmount (char_skill.cpp:1143)
            let mut rng = self.world.resource_mut::<Rand>();
            let mut roll = |lo: i32, hi: i32| rng.roll(lo, hi);
            let damage = skill_damage(proto.attr_type, amount, victim_def, &mut roll);
            if damage > 0 {
                let flag = damage_flag_for_attr(proto.attr_type);
                let pkt = protocol::combat::GcDamageInfo::new(target_vid, flag, damage).to_bytes().to_vec();
                let mut dead = false;
                let mut hp_after = 0;
                // La vista del mob ANTES del daño (parity FuncSplashDamage:
                // el KillInfo del kill se captura antes de destruir el mob —
                // si no, `remove_npc` la borra y el canal salta la recompensa
                // de exp/gold del kill single-target; el splash ya lo hace
                // bien).
                let view = self.npc_view(target_vid);
                if let Some(dmg) = self.damage_npc(target_vid, damage, Some(pe)) {
                    dead = dmg.dead;
                    hp_after = dmg.hp;
                    if dead {
                        self.remove_npc(target_vid);
                    }
                }
                let victim = view.map(|v| KillInfo {
                    vnum: v.vnum,
                    x: v.state.x,
                    y: v.state.y,
                    hp: hp_after,
                    max_hp: v.max_hp,
                    exp: v.exp,
                    gold_min: v.gold_min,
                    gold_max: v.gold_max,
                    drop_item: v.drop_item,
                    mob_level: v.state.level,
                });
                events.push(SkillEvent::SkillResult {
                    player_vid,
                    skill_id,
                    victim_vid: target_vid,
                    packets: vec![pkt],
                    damage,
                    dead,
                    victim,
                    sp_cost,
                    hp_cost,
                    buff: None,
                }.into());
            } else {
                // Daño 0 (bloqueado): el skill se gastó (SP/cooldown) — el
                // canal solo recibe el coste (GC_POINTS).
                events.push(SkillEvent::SkillResult {
                    player_vid,
                    skill_id,
                    victim_vid: target_vid,
                    packets: Vec::new(),
                    damage: 0,
                    dead: false,
                    victim: None,
                    sp_cost,
                    hp_cost,
                    buff: None,
                }.into());
            }
            } // cierra la rama single-target (rama else del SPLASH)
        } else {
            // Buff (parity ComputeSkill: iDur > 0 → AddAffect; el valor del
            // poly se aplica y el icono se manda al cliente).
            if amount != 0 && duration_secs > 0 {
                let value = amount;
                // Aplicar los buffs de pools YA (parity PointChange(MAX_HP/SP)
                // — el máximo sube y el actual lo acompaña).
                if let Ok(mut ent) = self.world.get_entity_mut(buff_target) {
                    match proto.point_on {
                        point::MAX_HP => {
                            if let Some(mut h) = ent.get_mut::<Hp>() {
                                h.max_hp = (h.max_hp + value).max(0);
                                h.hp = (h.hp + value).min(h.max_hp);
                            }
                        }
                        point::MAX_SP => {
                            if let Some(mut m) = ent.get_mut::<Mp>() {
                                m.max_mp = (m.max_mp + value).max(0);
                                m.mp = (m.mp + value).min(m.max_mp);
                            }
                        }
                        _ => {}
                    }
                    if let Some(mut affects) = ent.get_mut::<Affects>() {
                        affects.0.push(Affect {
                            skill_id,
                            point: proto.point_on,
                            value,
                            flag: proto.affect_flag,
                            duration_ms: u64::from(duration_secs.max(0) as u32) * 1000,
                            sp_cost: 0,
                        });
                    }
                }
                events.push(SkillEvent::SkillResult {
                    player_vid,
                    skill_id,
                    victim_vid: target_vid,
                    packets: Vec::new(),
                    damage: 0,
                    dead: false,
                    victim: None,
                    sp_cost,
                    hp_cost,
                    buff: Some(protocol::world::TPacketAffectElement {
                        dw_type: skill_id,
                        b_apply_on: proto.point_on,
                        l_apply_value: value,
                        dw_flag: proto.affect_flag,
                        l_duration: duration_secs,
                        l_sp_cost: 0,
                    }),
                }.into());
            } else {
                // Sin efecto (poly 0 o sin duración) — solo el coste.
                events.push(SkillEvent::SkillResult {
                    player_vid,
                    skill_id,
                    victim_vid: target_vid,
                    packets: Vec::new(),
                    damage: 0,
                    dead: false,
                    victim: None,
                    sp_cost,
                    hp_cost,
                    buff: None,
                }.into());
            }
        }
        events
    }

    /// Modo SPLASH (área — flag SKILL_FLAG_SPLASH; parity `ComputeSkill`
    /// char_skill.cpp:2095-2104 + `FuncSplashDamage::OnHit` 1028-1468 +
    /// `ComputeSkillAtPosition` 1599-1735): el centro ya resuelto, el radio
    /// `iSplashRange` y el máx `lMaxHit` (0 = sin límite — parity
    /// `lMaxHit ? lMaxHit : -1`, char_skill.cpp:2464-2465). Cada víctima
    /// dentro del radio (mob vivo o PC atacable) recibe SU daño: el poly se
    /// re-evalúa con SUS vars (`atk` vs víctima, `ar` vs víctima, `maxhp`/
    /// `maxsp` DE la víctima — parity FuncSplashDamage 1097-1121); las
    /// víctimas que NO son el main target se multiplican por
    /// `kSplashAroundDamageAdjustPoly` (char_skill.cpp:1206-1209 — ANTES
    /// del ajuste por attr). Mobs SIEMPRE atacables (parity
    /// `battle_is_attackable`: `pkVictim->IsNPC() || pkChr->IsNPC()` →
    /// true, pvp.cpp:430); PCs → el gate PvP del mundo; el caster nunca es
    /// víctima (`pkChr == pkVictim` → false, pvp.cpp:383-384). El orden de
    /// iteración es vid ASC (desviación documentada: el ForEachAround del
    /// C++ no garantiza orden — el orden estable hace el `lMaxHit`
    /// determinista). El coste SP/cooldown ya se pagó UNA vez (el caller).
    /// Emite `SplashResult` (al caster: TODOS los paquetes + los kills) y
    /// un `SplashVictimHit` por PC golpeado (routing a la víctima — parity
    /// SendDamagePacket, char_battle.cpp:1508-1527).
    #[allow(clippy::too_many_arguments)]
    fn splash_damage(
        &mut self,
        player_vid: u32,
        pe: Entity,
        skill_id: u32,
        proto: &SkillProto,
        center: (i32, i32),
        main_target_vid: u32,
        weapon: Option<&ProtoItem>,
        sp_cost: i32,
        hp_cost: i32,
        k: f64,
        level: i32,
        job: u8,
        st: i32,
        dx: i32,
        iq: i32,
        ht: i32,
        armor: i32,
    ) -> Vec<NpcEvent> {
        // Candidatos: mobs materializados vivos + jugadores vivos (el
        // caster NO — parity `pkChr == pkVictim → false`). (vid, x, y, pc).
        let mut candidates: Vec<(u32, i32, i32, bool)> = Vec::new();
        for (&vid, &e) in &self.world.resource::<NpcIndex>().0 {
            let Ok(ent) = self.world.get_entity(e) else { continue };
            let (Some(pos), Some(hp)) = (ent.get::<Position>(), ent.get::<Hp>()) else { continue };
            if hp.hp > 0 {
                candidates.push((vid, pos.x, pos.y, false));
            }
        }
        for (&vid, &e) in &self.players {
            if vid == player_vid {
                continue;
            }
            let Ok(ent) = self.world.get_entity(e) else { continue };
            let (Some(pos), Some(hp)) = (ent.get::<Position>(), ent.get::<Hp>()) else { continue };
            if hp.hp > 0 {
                candidates.push((vid, pos.x, pos.y, true));
            }
        }
        candidates.sort_unstable_by_key(|c| c.0);

        let player_state = PlayerState {
            vid: player_vid,
            x: center.0,
            y: center.1,
            level,
            ht,
            job,
            st,
            dx,
            iq,
            attack_speed_ms: attack_speed_for_weapon(weapon),
            att_grade_bonus: 0,
            critical_pct: 0,
        };
        let def = f64::from(player_def_grade(level, ht, armor));
        let odef = def;
        let splash_range = proto.splash_range as i32;
        let max_hit = if proto.max_hit == 0 { usize::MAX } else { proto.max_hit as usize };
        let adjust_around = !proto.splash_adjust_poly.trim().is_empty();
        let mut hits = 0usize;
        let mut victims: Vec<SplashVictimInfo> = Vec::new();
        let mut events: Vec<NpcEvent> = Vec::new();

        for (vid, vx, vy, is_pc) in candidates {
            if hits >= max_hit {
                break;
            }
            // Radio (parity FuncSplashDamage 1041-1046: `dist > range` → no).
            if distance_approx(center.0 - vx, center.1 - vy) > splash_range {
                continue;
            }
            // Gate de atacabilidad: mob → siempre (pvp.cpp:430 — `IsNPC()`
            // del C++); PC → el gate PvP del mundo (pk mode/party/muerto).
            let ve = if is_pc { self.players.get(&vid).copied() } else { None };
            if let Some(ve) = ve
                && !self.pvp_attackable(pe, ve) {
                    continue;
                }
            hits += 1; // el slot del lMaxHit se consume (parity HitOnce)

            // Stats de la víctima (las vars del poly — parity
            // FuncSplashDamage 1097-1121: `atk`/`ar` contra la víctima,
            // `maxhp`/`maxsp` DE la víctima).
            let (victim_dx, victim_lv, victim_def, vmax_hp, vmax_sp) = if let Some(ve) = ve {
                let Ok(ent) = self.world.get_entity(ve) else { continue };
                let (Some(p), Some(h), Some(m)) =
                    (ent.get::<Player>(), ent.get::<Hp>(), ent.get::<Mp>())
                else { continue };
                (p.dx, p.level, player_def_grade(p.level, p.ht, p.armor), h.max_hp, m.max_mp)
            } else {
                let Some(view) = self.npc_view(vid) else { continue };
                let s = view.state;
                (s.dx, s.level, def_grade_npc(s.level, s.ht, s.wdef), view.max_hp, 0)
            };

            // Daño del poly re-evaluado con las vars de la víctima + el
            // ajuste del splash (el main target NO se ajusta).
            let damage = {
                let mut rng = self.world.resource_mut::<Rand>();
                let mut roll = |lo: i32, hi: i32| rng.roll(lo, hi);
                let atk = attack_power(&player_state, victim_dx, victim_lv, weapon, &mut roll) as f64;
                let ar = calc_attack_rating(dx, level, victim_dx, victim_lv) as f64;
                let var = |name: &str| -> Option<f64> {
                    match name {
                        "k" => Some(k),
                        "atk" => Some(atk),
                        "lv" => Some(f64::from(level)),
                        "iq" => Some(f64::from(iq)),
                        "str" => Some(f64::from(st)),
                        "dex" => Some(f64::from(dx)),
                        "con" => Some(f64::from(ht)),
                        "maxhp" => Some(f64::from(vmax_hp)),
                        "maxsp" => Some(f64::from(vmax_sp)),
                        "ar" => Some(ar),
                        "def" => Some(def),
                        "odef" => Some(odef),
                        _ => None,
                    }
                };
                let amount = eval_poly(&proto.point_poly, &var, &mut roll).unwrap_or(0.0) as i32;
                // CalcBattleDamage floor (battle.cpp:199-206).
                let mut damage = (-amount).max(0);
                if damage < 3 {
                    damage = roll(1, 5);
                }
                // kSplashAroundDamageAdjustPoly (char_skill.cpp:1206-1209 —
                // ANTES del ajuste por attr).
                if vid != main_target_vid && adjust_around {
                    let adj = eval_poly(&proto.splash_adjust_poly, &var, &mut roll).unwrap_or(1.0);
                    damage = (damage as f64 * adj) as i32;
                }
                damage
            };
            // Ajuste por attr (parity char_skill.cpp:1211-1246): MELEE →
            // -victim DEF_GRADE (RANGE/MAGIC: resistencias 0 — identidad).
            let damage = if proto.attr_type == attr_type::MELEE {
                damage.saturating_sub(victim_def).max(0)
            } else {
                damage
            };
            if damage <= 0 {
                continue; // bloqueado — el slot del lMaxHit ya se consumió
            }

            let flag = damage_flag_for_attr(proto.attr_type);
            let pkt = protocol::combat::GcDamageInfo::new(vid, flag, damage).to_bytes().to_vec();
            if let Some(ve) = ve {
                // PC: el daño va al Hp del mundo y la víctima recibe su
                // evento (GC_DAMAGE_INFO + GC_POINTS/GC_DEAD en el canal).
                let hp_after = {
                    let Ok(mut ent) = self.world.get_entity_mut(ve) else { continue };
                    let Some(mut h) = ent.get_mut::<Hp>() else { continue };
                    h.hp = (h.hp - damage).max(0);
                    h.hp
                };
                let dead = hp_after <= 0;
                events.push(
                    SkillEvent::SplashVictimHit {
                        player_vid: vid,
                        attacker_vid: player_vid,
                        packets: vec![pkt.clone()],
                        damage,
                        dead,
                    }
                    .into(),
                );
                victims.push(SplashVictimInfo {
                    victim_vid: vid,
                    packets: vec![pkt],
                    damage,
                    dead,
                    victim: None,
                });
            } else {
                // Mob: la vista ANTES del daño (el KillInfo del kill — el
                // despawn la invalidaría).
                let view = self.npc_view(vid);
                let mut dead = false;
                let mut hp_after = 0;
                if let Some(dmg) = self.damage_npc(vid, damage, Some(pe)) {
                    dead = dmg.dead;
                    hp_after = dmg.hp;
                    if dead {
                        self.remove_npc(vid);
                    }
                }
                let victim = view.map(|v| KillInfo {
                    vnum: v.vnum,
                    x: v.state.x,
                    y: v.state.y,
                    hp: hp_after,
                    max_hp: v.max_hp,
                    exp: v.exp,
                    gold_min: v.gold_min,
                    gold_max: v.gold_max,
                    drop_item: v.drop_item,
                    mob_level: v.state.level,
                });
                victims.push(SplashVictimInfo {
                    victim_vid: vid,
                    packets: vec![pkt],
                    damage,
                    dead,
                    victim,
                });
            }
        }
        // El caster ve TODOS los paquetes y paga el coste UNA vez (el
        // canal aplica el SP/cooldown con este evento).
        events.push(
            SkillEvent::SplashResult {
                player_vid,
                skill_id,
                victims,
                sp_cost,
                hp_cost,
            }
            .into(),
        );
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::events::{CombatEvent, CombatIntent, NpcEvent, SkillEvent, SkillIntent};
    use crate::ecs::test_util::*;
    use crate::skill::SkillProto;

    /// El proto REAL del skill 1 (삼연참 — warrior; skill_proto del runtime).
    fn skill1_proto() -> SkillProto {
        SkillProto {
            vnum: 1,
            b_type: 0,
            level_step: 1,
            max_level: 40,
            point_on: crate::skill::point_from_text("HP").unwrap(),
            point_poly: "-( 1.1*atk + (0.5*atk +  1.5 * str)*k)".into(),
            sp_cost_poly: "80+220*k".into(),
            duration_poly: String::new(),
            cooldown_poly: "12".into(),
            flag: crate::skill::skill_flags_from_text("ATTACK,USE_MELEE_DAMAGE"),
            affect_flag: 0,
            attr_type: crate::skill::attr_type::MELEE,
            max_hit: 1,
            target_range: 0,
            splash_range: 0,
            splash_adjust_poly: String::new(),
        }
    }

    /// El proto REAL del skill 1 (삼연참) en modo SPLASH — los valores del
    /// skill_proto del runtime: `dwsplashrange` 250, `imaxhit` 4,
    /// `szsplasharounddamageadjustpoly` "0.5" (el daño de las víctimas
    /// alrededor del main target se reduce a la mitad).
    fn skill1_splash_proto() -> SkillProto {
        SkillProto {
            vnum: 1,
            b_type: 0,
            level_step: 1,
            max_level: 40,
            point_on: crate::skill::point_from_text("HP").unwrap(),
            point_poly: "-( 1.1*atk + (0.5*atk +  1.5 * str)*k)".into(),
            sp_cost_poly: "80+220*k".into(),
            duration_poly: String::new(),
            cooldown_poly: "12".into(),
            flag: crate::skill::skill_flags_from_text("ATTACK,USE_MELEE_DAMAGE,SPLASH"),
            affect_flag: 0,
            attr_type: crate::skill::attr_type::MELEE,
            max_hit: 4,
            target_range: 0,
            splash_range: 250,
            splash_adjust_poly: "0.5".into(),
        }
    }

    /// El proto REAL del skill 19 (천근 — self-buff de DEF, CHEONGEUN).
    fn skill19_proto() -> SkillProto {
        SkillProto {
            vnum: 19,
            b_type: 0,
            level_step: 1,
            max_level: 40,
            point_on: crate::skill::point_from_text("DEF_GRADE").unwrap(),
            point_poly: "(200 + str*0.2 + con*0.5 ) *k".into(),
            sp_cost_poly: "80+220*k".into(),
            duration_poly: "60+90*k".into(),
            cooldown_poly: "63+90*k".into(),
            flag: crate::skill::skill_flags_from_text("SELFONLY"),
            affect_flag: crate::skill::affect_flag_from_text("CHEONGEUN"),
            attr_type: crate::skill::attr_type::NORMAL,
            max_hit: 1,
            target_range: 0,
            splash_range: 0,
            splash_adjust_poly: String::new(),
        }
    }

    fn load_skills(w: &mut WorldSim, protos: Vec<SkillProto>) {
        w.world.resource_mut::<SkillTable>().0 = protos.into_iter().map(|p| (p.vnum, p)).collect();
    }

    /// El ataque del skill 1 contra el mob 101: el daño del poly real
    /// (atk ≈ 56-57 sin arma → amount ≈ 90 → 90 − DEF 10 = 80-82), el SP
    /// coste pagado, el cooldown de 12 s impuesto (el segundo uso se rechaza
    /// en silencio — parity TSkillUseInfo).
    #[test]
    fn use_skill_attack_damages_mob_and_enforces_cooldown() {
        let mut w = world_with(42);
        load(&mut w, vec![(entry(101, 0, 0, 1), mob_row(101))]);
        join_with_skills(&mut w, 2, &[(1, 1)]);
        w.set_player_mp(2, 500);
        load_skills(&mut w, vec![skill1_proto()]);
        let events = w.process_intent(
            SkillIntent::UseSkill { player_vid: 2, skill_id: 1, target_vid: 10_000, weapon: None }.into(),
            1_000,
        );
        let result = events.iter().find_map(|e| match e {
            NpcEvent::Skill(SkillEvent::SkillResult { skill_id, packets, damage, dead, victim, sp_cost, hp_cost, buff, .. }) => {
                Some((*skill_id, packets.clone(), *damage, *dead, *victim, *sp_cost, *hp_cost, buff.clone()))
            }
            _ => None,
        });
        let (sid, packets, damage, dead, victim, sp_cost, hp_cost, buff) = result.expect("SkillResult");
        assert_eq!(sid, 1);
        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0][0], 135, "GC_DAMAGE_INFO");
        assert_eq!(packets[0][1..5], 10_000u32.to_le_bytes(), "dwVID del mob");
        assert_eq!(packets[0][5], crate::skill::damage_type::MELEE, "flag MELEE");
        assert!((80..=82).contains(&damage), "daño del poly vs mob 101: {damage}");
        assert!(!dead);
        assert!(sp_cost > 0, "SP coste: {sp_cost}");
        assert_eq!(hp_cost, 0);
        assert!(buff.is_none());
        let v = victim.expect("víctima del skill");
        assert_eq!(v.vnum, 101);
        // El mob recibió el daño (hp 126 − daño).
        assert_eq!(v.hp, 126 - damage);
        // Cooldown 12 s: el segundo uso inmediato → rechazo silencioso.
        assert!(
            w.process_intent(
                SkillIntent::UseSkill { player_vid: 2, skill_id: 1, target_vid: 10_000, weapon: None }.into(),
                1_100,
            )
            .is_empty(),
            "cooldown activo"
        );
        // Tras el cooldown (12 s) se puede usar de nuevo.
        assert!(
            !w.process_intent(
                SkillIntent::UseSkill { player_vid: 2, skill_id: 1, target_vid: 10_000, weapon: None }.into(),
                14_000,
            )
            .is_empty(),
            "cooldown expirado"
        );
    }

    /// El self-buff de DEF (skill 19 — CHEONGEUN): el buff se aplica (con el
    /// elemento del wire correcto), la DEF_GRADE bonus reduce el daño del mob
    /// y al expirar se revierte con `AffectRemoved`.
    #[test]
    fn use_skill_buff_applies_def_bonus_and_expires() {
        let mut w = world_with(42);
        let mut row = mob_row(101);
        row.ai_flag = Some("AGGR".into());
        row.damage_min = 200;
        row.damage_max = 200; // daño fijo: 200 − DEF del jugador
        load(&mut w, vec![(entry(101, 0, 0, 1), row)]);
        join_with_skills(&mut w, 2, &[(19, 1)]);
        w.set_player_mp(2, 500);
        load_skills(&mut w, vec![skill19_proto()]);

        // Sin buff: 200 − (5 + 24) = 171 (ninja sin armor). C29: cooldown
        // 2000 ms — un tick de 2000 ms dispara el primer golpe.
        let events = w.update(2_000);
        let d1 = events.iter().find_map(|e| match e {
            NpcEvent::Combat(CombatEvent::MobAttack { damage, .. }) => Some(*damage),
            _ => None,
        });
        assert_eq!(d1, Some(171), "200 − player_def_grade(5,30,0)");

        // Buff: valor = (200 + 6 + 15)*0.4 = 88; duración 96 s; AFF_CHEONGEUN.
        let events = w.process_intent(
            SkillIntent::UseSkill { player_vid: 2, skill_id: 19, target_vid: 2, weapon: None }.into(),
            1_000,
        );
        let result = events.iter().find_map(|e| match e {
            NpcEvent::Skill(SkillEvent::SkillResult { skill_id, damage, sp_cost, buff, .. }) => {
                Some((*skill_id, *damage, *sp_cost, buff.clone()))
            }
            _ => None,
        });
        let (sid, damage, sp_cost, buff) = result.expect("SkillResult");
        assert_eq!(sid, 19);
        assert_eq!(damage, 0, "buff sin daño");
        assert!(sp_cost > 0);
        let elem = buff.expect("buff aplicado");
        assert_eq!(elem.dw_type, 19, "dwType = skill vnum");
        assert_eq!(elem.b_apply_on, crate::skill::point::DEF_GRADE_BONUS);
        assert_eq!(elem.l_apply_value, 88, "(200+6+15)*0.4");
        assert_eq!(elem.dw_flag, crate::skill::aff::CHEONGEUN);
        assert_eq!(elem.l_duration, 96, "60+90*0.4");

        // Con el buff: 200 − (29 + 88) = 83. C29: el cooldown sigue activo
        // tras el skill (2000 ms desde el último golpe) — otro tick de 2000.
        let events = w.update(2_000);
        let d2 = events.iter().find_map(|e| match e {
            NpcEvent::Combat(CombatEvent::MobAttack { damage, .. }) => Some(*damage),
            _ => None,
        });
        assert_eq!(d2, Some(83), "200 − (def 29 + bonus 88)");

        // El buff expira (96 s / 500 ms = 192 ticks) → AffectRemoved + revert.
        let mut removed = false;
        for _ in 0..200 {
            let events = w.update(500);
            removed |= events.iter().any(|e| matches!(e, NpcEvent::Skill(SkillEvent::AffectRemoved { skill_id: 19, .. })));
        }
        assert!(removed, "AffectRemoved al expirar");
        let events = w.update(2_000); // C29: cooldown 2000 ms para el golpe
        let d3 = events.iter().find_map(|e| match e {
            NpcEvent::Combat(CombatEvent::MobAttack { damage, .. }) => Some(*damage),
            _ => None,
        });
        assert_eq!(d3, Some(171), "sin buff otra vez (revertido)");
    }

    /// Rechazos silenciosos (parity: el legacy devuelve false sin respuesta):
    /// sin nivel, sin SP, fuera de rango.
    #[test]
    fn use_skill_rejects_without_level_mp_or_range() {
        let mut w = world_with(42);
        load(&mut w, vec![(entry(101, 0, 0, 1), mob_row(101))]);
        join_with_skills(&mut w, 2, &[]); // sin skills
        w.set_player_mp(2, 500);
        load_skills(&mut w, vec![skill1_proto()]);
        // Sin nivel en la skill 1 → rechazo.
        assert!(
            w.process_intent(
                SkillIntent::UseSkill { player_vid: 2, skill_id: 1, target_vid: 10_000, weapon: None }.into(),
                1_000,
            )
            .is_empty(),
            "sin nivel"
        );
        // Con nivel pero sin SP (mp 10 < coste 168) → rechazo.
        join_with_skills(&mut w, 3, &[(1, 1)]);
        w.set_player_mp(3, 10);
        assert!(
            w.process_intent(
                SkillIntent::UseSkill { player_vid: 3, skill_id: 1, target_vid: 10_000, weapon: None }.into(),
                2_000,
            )
            .is_empty(),
            "sin SP"
        );
        // Skill de rango 800 contra un mob a 1000 → fuera de rango (parity
        // ComputeSkill: `dwTargetRange && dist >= range + 50`).
        let mut w2 = world_with(42);
        load(&mut w2, vec![(entry(101, 1_000, 0, 1), mob_row(101))]);
        join_with_skills(&mut w2, 2, &[(1, 1)]);
        w2.set_player_mp(2, 500);
        let mut ranged = skill1_proto();
        ranged.vnum = 32;
        ranged.target_range = 800;
        load_skills(&mut w2, vec![ranged]);
        assert!(
            w2.process_intent(
                SkillIntent::UseSkill { player_vid: 2, skill_id: 32, target_vid: 10_000, weapon: None }.into(),
                1_000,
            )
            .is_empty(),
            "mob a 1000 > 800+50"
        );
    }

    /// El skill sin fila en la SkillTable (tabla no cargada) → rechazo.
    #[test]
    fn use_skill_unknown_skill_is_rejected() {
        let mut w = world_with(42);
        join_with_skills(&mut w, 2, &[(1, 1)]);
        w.set_player_mp(2, 500);
        // Sin load_skills: la tabla está vacía.
        assert!(
            w.process_intent(
                SkillIntent::UseSkill { player_vid: 2, skill_id: 1, target_vid: 0, weapon: None }.into(),
                1_000,
            )
            .is_empty()
        );
    }

    /// El `k` del poly con la tabla REAL cargada (parity char_skill.cpp:1632
    /// — `k = GetSkillPower(vnum, level) * bMaxLevel / 100`): con la tabla
    /// inyectada, el k del assassin (job 1) group 2 (idx = 1×2+1 = 3) es
    /// 1003×40/100 = 401.2 — el daño del skill 1 deja de ser el de la
    /// aproximación (80-82) y refleja el poder de la tabla. Sin tabla
    /// (fail-open) → la aproximación (el test `use_skill_attack_...` ya lo
    /// cubre).
    #[test]
    fn skill_power_table_drives_k_in_poly() {
        let mut w = world_with(42);
        load(&mut w, vec![(entry(101, 0, 0, 1), mob_row(101))]);
        join_with_skills_group(&mut w, 2, &[(1, 1)], 2); // job 1 (assassin) group 2
        // El coste SP del poly sube con el k real (80+220×401.2 ≈ 88k).
        w.set_player_mp(2, 200_000);
        load_skills(&mut w, vec![skill1_proto()]);
        // Tabla real: fila i → 1000+i en cada nivel (idx 3 → 1003).
        let rows: Vec<[i32; 41]> = (0..9).map(|i| [1000 + i as i32; 41]).collect();
        w.world.resource_mut::<SkillPowerTable>().0 =
            std::sync::Arc::new(database::skill_power::SkillPowerTable::from_rows(rows));
        let events = w.process_intent(
            SkillIntent::UseSkill { player_vid: 2, skill_id: 1, target_vid: 10_000, weapon: None }.into(),
            1_000,
        );
        let damage = events.iter().find_map(|e| match e {
            NpcEvent::Skill(SkillEvent::SkillResult { damage, .. }) => Some(*damage),
            _ => None,
        });
        let damage = damage.expect("SkillResult");
        // k = 1003 × 40 / 100 = 401.2 → (0.5×atk + 1.5×30)×401 ≫ el daño de
        // la aproximación (k = 0.4 → 80-82). La tabla real DOMINA el poly.
        assert!(
            damage > 1_000,
            "el k de la tabla (401.2) debe dominar el daño, no la aproximación: {damage}"
        );
        // Contraste: sin tabla (fail-open) el mismo escenario da 80-82.
        let mut w2 = world_with(42);
        load(&mut w2, vec![(entry(101, 0, 0, 1), mob_row(101))]);
        join_with_skills_group(&mut w2, 2, &[(1, 1)], 2);
        w2.set_player_mp(2, 500);
        load_skills(&mut w2, vec![skill1_proto()]);
        let events = w2.process_intent(
            SkillIntent::UseSkill { player_vid: 2, skill_id: 1, target_vid: 10_000, weapon: None }.into(),
            1_000,
        );
        let damage = events.iter().find_map(|e| match e {
            NpcEvent::Skill(SkillEvent::SkillResult { damage, .. }) => Some(*damage),
            _ => None,
        });
        assert!((80..=82).contains(&damage.expect("SkillResult sin tabla")), "fail-open → aproximación");
    }

    /// SPLASH (área — skill 1 con flag SPLASH, radio 250, lMaxHit 4): el
    /// target del wire define el CENTRO — 3 mobs a (0,0), (100,0) y (300,0):
    /// los 2 primeros caen dentro del radio, el tercero fuera. Cada víctima
    /// recibe su GC_DAMAGE_INFO (el main target SIN el ajuste del splash;
    /// las demás ×0.5 — `szSplashAroundDamageAdjustPoly` — ANTES de la DEF)
    /// y el SP/cooldown se pagan UNA vez por uso (el segundo uso inmediato
    /// se rechaza — parity TSkillUseInfo).
    #[test]
    fn splash_hits_all_mobs_in_radius_and_pays_cost_once() {
        let mut w = world_with(42);
        let nomove = |v: i64| {
            let mut r = mob_row(v);
            r.ai_flag = Some("NOMOVE".into()); // determinista: no patrullan
            r
        };
        load(&mut w, vec![
            (entry(101, 0, 0, 1), nomove(101)),
            (entry(102, 100, 0, 1), nomove(102)),
            (entry(103, 300, 0, 1), nomove(103)), // a ~288 del centro: fuera del radio 250
        ]);
        join_with_skills(&mut w, 2, &[(1, 1)]);
        w.set_player_mp(2, 500);
        load_skills(&mut w, vec![skill1_splash_proto()]);
        let events = w.process_intent(
            SkillIntent::UseSkill { player_vid: 2, skill_id: 1, target_vid: 10_000, weapon: None }.into(),
            1_000,
        );
        let result = events.iter().find_map(|e| match e {
            NpcEvent::Skill(SkillEvent::SplashResult { skill_id, victims, sp_cost, hp_cost, .. }) => {
                Some((*skill_id, victims.clone(), *sp_cost, *hp_cost))
            }
            _ => None,
        });
        let (sid, victims, sp_cost, hp_cost) = result.expect("SplashResult");
        assert_eq!(sid, 1);
        assert_eq!(
            victims.len(),
            2,
            "mobs 101 y 102 dentro del radio; el 103 a ~288 > 250: {victims:?}"
        );
        assert!(sp_cost > 0, "SP coste UNA vez: {sp_cost}");
        assert_eq!(hp_cost, 0);
        // Orden estable por vid: 10_000 (main target — sin ajuste) y 10_001.
        let v0 = &victims[0];
        let v1 = &victims[1];
        assert_eq!(v0.victim_vid, 10_000);
        assert_eq!(v1.victim_vid, 10_001);
        assert_eq!(v0.packets.len(), 1);
        assert_eq!(v0.packets[0][0], 135, "GC_DAMAGE_INFO");
        assert_eq!(v0.packets[0][1..5], 10_000u32.to_le_bytes(), "dwVID");
        assert_eq!(v0.packets[0][5], crate::skill::damage_type::MELEE, "flag MELEE");
        // El main target: el MISMO daño que el single-target (mismo atk vs
        // el mob 101 — 80-82). Las demás: floor × 0.5 ANTES de −DEF 10
        // (90-92 → 45-46 → 35-36).
        assert!((80..=82).contains(&v0.damage), "main target sin ajuste: {}", v0.damage);
        assert!((35..=36).contains(&v1.damage), "ajuste 0.5 antes de la DEF: {}", v1.damage);
        assert!(!v0.dead && !v1.dead);
        let k0 = v0.victim.expect("kill info del mob 101");
        assert_eq!(k0.vnum, 101);
        assert_eq!(k0.hp, 126 - v0.damage);
        let k1 = v1.victim.expect("kill info del mob 102");
        assert_eq!(k1.vnum, 102);
        assert_eq!(k1.hp, 126 - v1.damage);
        // Cooldown 12 s: el segundo uso inmediato → rechazo silencioso
        // (el cooldown se paga UNA vez por uso, no por víctima).
        assert!(
            w.process_intent(
                SkillIntent::UseSkill { player_vid: 2, skill_id: 1, target_vid: 10_000, weapon: None }.into(),
                1_100,
            )
            .is_empty(),
            "cooldown activo"
        );
    }

    /// SPLASH con `lMaxHit` 2 (el imaxhit del skill_proto): solo las 2
    /// primeras víctimas (orden por vid — desviación documentada vs el
    /// ForEachAround sin orden) reciben daño; el mob restante queda intacto.
    #[test]
    fn splash_max_hit_limits_victims() {
        let mut w = world_with(42);
        let nomove = |v: i64| {
            let mut r = mob_row(v);
            r.ai_flag = Some("NOMOVE".into());
            r
        };
        load(&mut w, vec![
            (entry(101, 0, 0, 1), nomove(101)),
            (entry(102, 50, 0, 1), nomove(102)),
            (entry(103, 100, 0, 1), nomove(103)),
        ]);
        join_with_skills(&mut w, 2, &[(1, 1)]);
        w.set_player_mp(2, 500);
        let mut proto = skill1_splash_proto();
        proto.max_hit = 2;
        load_skills(&mut w, vec![proto]);
        let events = w.process_intent(
            SkillIntent::UseSkill { player_vid: 2, skill_id: 1, target_vid: 10_000, weapon: None }.into(),
            1_000,
        );
        let victims = events.iter().find_map(|e| match e {
            NpcEvent::Skill(SkillEvent::SplashResult { victims, .. }) => Some(victims.clone()),
            _ => None,
        });
        let victims = victims.expect("SplashResult");
        assert_eq!(victims.len(), 2, "lMaxHit 2: la tercera víctima no recibe daño");
        assert_eq!(victims[0].victim_vid, 10_000);
        assert_eq!(victims[1].victim_vid, 10_001);
        assert_eq!(w.npc_view(10_002).expect("mob 103").hp, 126, "intacto");
    }

    /// SPLASH: (a) sin target válido el CENTRO es el caster (parity —
    /// `pkVictim` null → la posición del caster): el mob 101 a (0,0) cae
    /// dentro; (b) el gate PvP del mundo: un PC con PK ON dentro del radio
    /// recibe su `SplashVictimHit` (routing a la VÍCTIMA — parity
    /// SendDamagePacket) y un PC con PK OFF no; (c) el caster NUNCA es
    /// víctima (parity `pkChr == pkVictim` → false, pvp.cpp:383-384).
    #[test]
    fn splash_center_fallback_and_pvp_gate() {
        let mut w = world_with(42);
        let mut row = mob_row(101);
        row.ai_flag = Some("NOMOVE".into());
        load(&mut w, vec![(entry(101, 0, 0, 1), row)]);
        join_with_skills(&mut w, 2, &[(1, 1)]); // caster en (0,0)
        join_at(&mut w, 3, 0, 100); // PC víctima cerca (PK on)
        join_at(&mut w, 4, 0, 200); // PC víctima (PK off — no atacable)
        w.set_player_mp(2, 500);
        w.process_intent(
            CombatIntent::SetPvpMode { player_vid: 3, on: true }.into(),
            1_000,
        );
        load_skills(&mut w, vec![skill1_splash_proto()]);
        // target_vid inválido (9999): centro = caster (0,0).
        let events = w.process_intent(
            SkillIntent::UseSkill { player_vid: 2, skill_id: 1, target_vid: 9_999, weapon: None }.into(),
            2_000,
        );
        let victims = events.iter().find_map(|e| match e {
            NpcEvent::Skill(SkillEvent::SplashResult { victims, .. }) => Some(victims.clone()),
            _ => None,
        });
        let victims = victims.expect("SplashResult");
        let vids: Vec<u32> = victims.iter().map(|v| v.victim_vid).collect();
        assert_eq!(
            vids,
            vec![3, 10_000],
            "PC 3 (PK on) + mob 101 — orden por vid (3 < 10_000)"
        );
        // El golpe al PC 3: evento con routing a la VÍCTIMA + el mundo ya
        // aplicó el daño a su Hp.
        let hit = events.iter().find_map(|e| match e {
            NpcEvent::Skill(SkillEvent::SplashVictimHit { player_vid, attacker_vid, damage, dead, .. }) => {
                Some((*player_vid, *attacker_vid, *damage, *dead))
            }
            _ => None,
        });
        let (victim_pvid, attacker, damage, dead) = hit.expect("SplashVictimHit");
        assert_eq!(victim_pvid, 3, "routing a la víctima");
        assert_eq!(attacker, 2);
        assert!(damage > 0);
        assert!(!dead, "hp 100 del PC 3");
        assert_eq!(w.player_hp(3), 100 - damage, "el mundo aplicó el daño al PC");
        // Sin evento para el PC 4 (PK off) ni para el caster.
        assert!(
            !events.iter().any(|e| matches!(e, NpcEvent::Skill(SkillEvent::SplashVictimHit { player_vid: 4, .. }))),
            "PK off: no atacable"
        );
        assert!(
            !events.iter().any(|e| matches!(e, NpcEvent::Skill(SkillEvent::SplashVictimHit { player_vid: 2, .. }))),
            "el caster no es víctima"
        );
    }
}
