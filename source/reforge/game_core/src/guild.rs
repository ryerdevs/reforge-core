//! F4 slice guild (2026-08-13): dominio PURO del ciclo de vida básico y de
//! los grades de una guild — structs + validaciones, sin I/O. La persistencia
//! ya existe (`database::social::GuildRepo`; `guild_grade` migrada) y el
//! dispatch wire (CG_GUILD_*) es un slice futuro.
//!
//! Parity: `GUILD_NAME_MAX_LEN = 12` (common/length.h:35); mínimo 2 chars =
//! diálogo del cliente (spec); duplicado: COUNT(*) de guild_manager.cpp:90-107.
//! Grades: 15 slots 1..=15 (`GUILD_GRADE_COUNT`, guild.h:11), nombre <=8
//! (`GUILD_GRADE_NAME_MAX_LEN`, guild.h:10), auth bitmask (guild.h:92-95).

pub const NAME_MAX: usize = 12; // GUILD_NAME_MAX_LEN, common/length.h:35
pub const NAME_MIN: usize = 2; // mínimo del diálogo cliente (spec slice)
pub const GRADE_COUNT: u8 = 15; // GUILD_GRADE_COUNT, guild.h:11
pub const GRADE_NAME_MAX: usize = 8; // GUILD_GRADE_NAME_MAX_LEN, guild.h:10

/// Miembro (clave natural `player.guild_member.player_id`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuildMember { pub player_id: i64 }

/// Guild en memoria (`player.guild.id` / `player.guild_member` / `guild_grade`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Guild { pub id: i64, pub name: String, pub members: Vec<GuildMember>, pub grades: Vec<GuildGrade> }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuildError { NameTooShort, NameTooLong, DuplicateName, DuplicateMember, DuplicateGrade, GradeFull }

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
    Ok(Guild { id, name: name.to_owned(), members: Vec::new(), grades: Vec::new() })
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
}