//! F4 slice guild (2026-08-13): dominio PURO del ciclo de vida básico, grades
//! y tablón de comentarios de una guild — structs + validaciones, sin I/O. La
//! persistencia ya existe (`database::social::GuildRepo`; `guild_grade`
//! migrada) y el dispatch wire (CG_GUILD_*) es un slice futuro.
//!
//! Parity: `GUILD_NAME_MAX_LEN = 12` (common/length.h:35); mínimo 2 chars =
//! diálogo del cliente (spec); duplicado: COUNT(*) de guild_manager.cpp:90-107.
//! Grades: 15 slots 1..=15 (`GUILD_GRADE_COUNT`, guild.h:11), nombre <=8
//! (`GUILD_GRADE_NAME_MAX_LEN`, guild.h:10), auth bitmask (guild.h:92-95).

pub const NAME_MAX: usize = 12; // GUILD_NAME_MAX_LEN, common/length.h:35
pub const NAME_MIN: usize = 2; // mínimo del diálogo cliente (spec slice)
pub const GRADE_COUNT: u8 = 15; // GUILD_GRADE_COUNT, guild.h:11
pub const GRADE_NAME_MAX: usize = 8; // GUILD_GRADE_NAME_MAX_LEN, guild.h:10
pub const COMMENT_MAX: usize = 50; // GUILD_COMMENT_MAX_LEN, guild.h:13

/// Miembro (clave natural `player.guild_member.player_id`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuildMember { pub player_id: i64 }

/// Comentario del tablón (`guild_comment.id/name/content`; `notice`='!' inicial
/// excluido del slice).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuildComment { pub id: i64, pub author: String, pub text: String }

/// Guild en memoria (`player.guild.id` / `player.guild_member` / `guild_grade`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Guild { pub id: i64, pub name: String, pub members: Vec<GuildMember>, pub grades: Vec<GuildGrade>, pub comments: Vec<GuildComment> }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuildError { NameTooShort, NameTooLong, DuplicateName, DuplicateMember, DuplicateGrade, GradeFull, EmptyComment, CommentTooLong, DuplicateComment }

/// `create_guild`: valida nombre (2..=12 tras trim) y duplicado
/// (case-insensitive) contra `existing`. Parity: guild_manager.cpp:90-107.
pub fn create_guild(id: i64, name: &str, existing: &[&str]) -> Result<Guild, GuildError> {
    let name = name.trim();
    let len = name.chars().count();
    if !(NAME_MIN..=NAME_MAX).contains(&len) {
        return Err(if len < NAME_MIN { GuildError::NameTooShort } else { GuildError::NameTooLong });
    }
    if existing.iter().any(|n| n.eq_ignore_ascii_case(name)) {
        return Err(GuildError::DuplicateName);
    }
    Ok(Guild { id, name: name.to_owned(), members: Vec::new(), grades: Vec::new(), comments: Vec::new() })
}

/// `add_member`: rechaza un `player_id` ya miembro (sin duplicados).
pub fn add_member(guild: &mut Guild, player_id: i64) -> Result<(), GuildError> {
    if guild.members.iter().any(|m| m.player_id == player_id) {
        return Err(GuildError::DuplicateMember);
    }
    guild.members.push(GuildMember { player_id });
    Ok(())
}

/// `remove_member`: `false` si el `player_id` no era miembro.
pub fn remove_member(guild: &mut Guild, player_id: i64) -> bool {
    let before = guild.members.len();
    guild.members.retain(|m| m.player_id != player_id);
    guild.members.len() != before
}

/// Grade 1..=15 (1 = líder); `auth` bitmask (guild.h:92-95): ADD_MEMBER=1,
/// REMOVE_MEMEBER=2, NOTICE=4, USE_SKILL=8 (typo `REMOVE_MEMEBER` legacy
/// preservado). Parity struct: guild.h:73-77,106 + load guild.cpp:563-566.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuildGrade { pub grade: u8, pub name: String, pub auth: u8 }

/// `add_grade`: asigna el primer slot libre 1..=15 (el C++ pre-inserta el
/// grade 1 del líder en create, guild.cpp:104-111 — aquí el slot se asigna
/// desde 1); nombre 1..=8 tras trim; Err si el nombre ya existe (verifier)
/// o el roster está lleno. Devuelve el slot asignado.
pub fn add_grade(guild: &mut Guild, name: &str) -> Result<u8, GuildError> {
    let name = name.trim();
    let len = name.chars().count();
    if !(1..=GRADE_NAME_MAX).contains(&len) {
        return Err(if len == 0 { GuildError::NameTooShort } else { GuildError::NameTooLong });
    }
    if guild.grades.iter().any(|g| g.name.eq_ignore_ascii_case(name)) {
        return Err(GuildError::DuplicateGrade);
    }
    let grade = (1..=GRADE_COUNT)
        .find(|n| guild.grades.iter().all(|g| g.grade != *n))
        .ok_or(GuildError::GradeFull)?;
    guild.grades.push(GuildGrade { grade, name: name.to_owned(), auth: 0 });
    Ok(grade)
}

/// `set_grade_auth`: auth bitmask en el slot (parity ChangeGradeAuth,
/// guild.cpp:827-842). `false` si el slot no existe.
pub fn set_grade_auth(guild: &mut Guild, grade: u8, auth: u8) -> bool {
    match guild.grades.iter_mut().find(|g| g.grade == grade) {
        Some(g) => { g.auth = auth; true }
        None => false,
    }
}

/// `add_comment`: texto 1..=COMMENT_MAX chars tras trim (parity max
/// guild.cpp:1006; C++ cuenta bytes, el slice cuenta chars). Rechaza vacío y
/// duplicado (mismo autor + mismo texto — el C++ NO lo comprueba, divergencia
/// deliberada del slice). Devuelve el id asignado (max+1, único entre vivos).
pub fn add_comment(guild: &mut Guild, author: &str, text: &str) -> Result<i64, GuildError> {
    let text = text.trim();
    let len = text.chars().count();
    if !(1..=COMMENT_MAX).contains(&len) {
        return Err(if len == 0 { GuildError::EmptyComment } else { GuildError::CommentTooLong });
    }
    if guild.comments.iter().any(|c| c.author == author && c.text == text) {
        return Err(GuildError::DuplicateComment);
    }
    let id = guild.comments.iter().map(|c| c.id).max().unwrap_or(0) + 1;
    guild.comments.push(GuildComment { id, author: author.to_owned(), text: text.to_owned() });
    Ok(id)
}

/// `remove_comment`: `false` si el id no existe (parity DeleteComment
/// id+guild, guild.cpp:1018-1025).
pub fn remove_comment(guild: &mut Guild, id: i64) -> bool {
    let before = guild.comments.len();
    guild.comments.retain(|c| c.id != id);
    guild.comments.len() != before
}

/// Guerra de guilds (stub): dos contendientes + marcador por lado. Parity:
/// score por par = `TEnemyGuild.score` vía SetWarScoreAgainstTo /
/// GetWarScoreAgainstTo, guild_war.cpp:178-232.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuildWar { pub guild_a: i64, pub guild_b: i64, pub score_a: u32, pub score_b: u32 }

/// `start_war`: abre la guerra con marcador 0-0 (stub: sin estados
/// GUILD_WAR_WAIT/ON_WAR — guild_war.cpp:20-23 es slice futuro).
pub fn start_war(guild_a: i64, guild_b: i64) -> GuildWar {
    GuildWar { guild_a, guild_b, score_a: 0, score_b: 0 }
}

/// `add_score`: guerra con `points` sumados al lado de `guild_id`
/// (funcional, sin mutación). `guild_id` ajeno a la guerra → copia sin
/// cambios — el C++ tampoco registra golpes de un par desconocido
/// (GetWarScoreAgainstTo → 0, guild_war.cpp:222-232).
pub fn add_score(war: GuildWar, guild_id: i64, points: u32) -> GuildWar {
    let mut w = war;
    if guild_id == w.guild_a { w.score_a += points; }
    else if guild_id == w.guild_b { w.score_b += points; }
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifier: FALLA si se permiten nombre vacío/corto/largo o duplicado.
    #[test]
    fn create_guild_rejects_invalid_names() {
        assert_eq!(create_guild(1, "", &[]), Err(GuildError::NameTooShort));
        assert_eq!(create_guild(1, "  ", &[]), Err(GuildError::NameTooShort));
        assert_eq!(create_guild(1, "a", &[]), Err(GuildError::NameTooShort));
        assert_eq!(create_guild(1, &"x".repeat(13), &[]), Err(GuildError::NameTooLong));
        assert_eq!(create_guild(1, "LosGuerreros", &["losguerreros"]), Err(GuildError::DuplicateName));
        assert_eq!(create_guild(1, "Valientes", &["OK"]).unwrap().name, "Valientes");
    }

    /// Verifier: FALLA si add_member admite duplicados o remove_member miente.
    #[test]
    fn member_flow_roundtrip() {
        let mut g = create_guild(1, "Valientes", &[]).unwrap();
        add_member(&mut g, 7).unwrap();
        assert_eq!(g.members, vec![GuildMember { player_id: 7 }]);
        assert_eq!(add_member(&mut g, 7), Err(GuildError::DuplicateMember));
        assert!(remove_member(&mut g, 7));
        assert!(!remove_member(&mut g, 7));
        assert!(g.members.is_empty());
    }

    /// Verifier: FALLA si add_grade admite un grade duplicado (mismo nombre,
    /// case-insensitive) o si set_grade_auth miente.
    #[test]
    fn add_grade_rejects_duplicates() {
        let mut g = create_guild(1, "Valientes", &[]).unwrap();
        assert_eq!(add_grade(&mut g, "Maestro"), Ok(1));
        assert_eq!(add_grade(&mut g, "MAESTRO"), Err(GuildError::DuplicateGrade));
        assert_eq!(add_grade(&mut g, "Miembro"), Ok(2));
        assert_eq!(add_grade(&mut g, "miembro"), Err(GuildError::DuplicateGrade));
        assert!(set_grade_auth(&mut g, 1, 1 | 4));
        assert_eq!(g.grades[0].auth, 5);
        assert!(!set_grade_auth(&mut g, 16, 0));
    }

    /// Verifier: FALLA si add_comment admite vacío, >50 chars o duplicado
    /// (mismo autor+texto), o si remove_comment miente.
    #[test]
    fn add_comment_rejects_empty_duplicate() {
        let mut g = create_guild(1, "Valientes", &[]).unwrap();
        assert_eq!(add_comment(&mut g, "Heroe", ""), Err(GuildError::EmptyComment));
        assert_eq!(add_comment(&mut g, "Heroe", "   "), Err(GuildError::EmptyComment));
        assert_eq!(add_comment(&mut g, "Heroe", &"x".repeat(51)), Err(GuildError::CommentTooLong));
        assert_eq!(add_comment(&mut g, "Heroe", &"x".repeat(50)), Ok(1));
        let id = add_comment(&mut g, "Heroe", "Bienvenidos!").unwrap();
        assert_eq!(add_comment(&mut g, "Heroe", "Bienvenidos!"), Err(GuildError::DuplicateComment));
        assert_eq!(add_comment(&mut g, "Otro", "Bienvenidos!"), Ok(id + 1));
        assert!(remove_comment(&mut g, id));
        assert!(!remove_comment(&mut g, id));
        assert_eq!(add_comment(&mut g, "Heroe", "Bienvenidos!"), Ok(id + 2)); // borrado → válido otra vez
    }

    /// Verifier: FALLA si start_war no abre 0-0, add_score cuenta mal o
    /// acepta una guild ajena a la guerra.
    #[test]
    fn guild_war_score_verifier() {
        assert_eq!(start_war(1, 2), GuildWar { guild_a: 1, guild_b: 2, score_a: 0, score_b: 0 });
        let w = add_score(start_war(1, 2), 1, 5);
        assert_eq!(w, GuildWar { guild_a: 1, guild_b: 2, score_a: 5, score_b: 0 });
        let w = add_score(add_score(w, 2, 3), 2, 2);
        assert_eq!(w.score_b, 5); // 3+2 acumulado en el lado B
        assert_eq!(add_score(w, 99, 1), w); // ajena → sin cambios
    }
}