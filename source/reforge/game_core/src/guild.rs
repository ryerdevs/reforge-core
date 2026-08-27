//! F4 slice guild (2026-08-13): dominio PURO del ciclo de vida básico de una
//! guild — structs + validaciones, sin I/O. La persistencia ya existe
//! (`database::social::GuildRepo`) y `guild_member/grade/comment` están
//! migradas; el dispatch wire (CG_GUILD_*) es un slice futuro.
//!
//! Parity: `GUILD_NAME_MAX_LEN = 12` (common/length.h:35); mínimo 2 chars =
//! diálogo del cliente (spec); duplicado: COUNT(*) de guild_manager.cpp:90-107.

pub const NAME_MAX: usize = 12; // GUILD_NAME_MAX_LEN, common/length.h:35
pub const NAME_MIN: usize = 2; // mínimo del diálogo cliente (spec slice)

/// Miembro (clave natural `player.guild_member.player_id`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuildMember { pub player_id: i64 }

/// Guild en memoria (`player.guild.id` / `player.guild_member`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Guild { pub id: i64, pub name: String, pub members: Vec<GuildMember> }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuildError { NameTooShort, NameTooLong, DuplicateName, DuplicateMember }

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
    Ok(Guild { id, name: name.to_owned(), members: Vec::new() })
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
}