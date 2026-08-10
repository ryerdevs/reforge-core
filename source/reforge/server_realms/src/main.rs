//! `server_realms` — binario único con roles (ADR-0004): un proceso por región.
//!
//! F2: rol `auth` (handshake + LOGIN3 + keys). F5: rol `channel` (realm).
//! Esqueleto mínimo: parseo de `--role` con std (sin clap todavía).

/// Rol del proceso.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    Auth,
    Channel,
}

/// Parsea el rol desde los args (sin el nombre del binario): `--role auth` |
/// `--role channel`. Default: `Auth`.
fn parse_role(args: &[String]) -> Result<Role, String> {
    let mut role = Role::Auth;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--role" => {
                let Some(v) = it.next() else {
                    return Err("--role requiere un valor (auth|channel)".into());
                };
                role = match v.as_str() {
                    "auth" => Role::Auth,
                    "channel" => Role::Channel,
                    other => return Err(format!("rol desconocido: {other} (auth|channel)")),
                };
            }
            other => return Err(format!("argumento desconocido: {other}")),
        }
    }
    Ok(role)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse_role(&args) {
        Ok(Role::Auth) => println!("server_realms role=auth — stub (F2)"),
        Ok(Role::Channel) => println!("server_realms role=channel — stub (F5)"),
        Err(e) => {
            eprintln!("server_realms: {e}");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_auth() {
        assert_eq!(parse_role(&[]), Ok(Role::Auth));
    }

    #[test]
    fn parses_explicit_roles() {
        assert_eq!(parse_role(&["--role".into(), "auth".into()]), Ok(Role::Auth));
        assert_eq!(
            parse_role(&["--role".into(), "channel".into()]),
            Ok(Role::Channel)
        );
    }

    #[test]
    fn rejects_invalid_role_and_args() {
        assert!(parse_role(&["--role".into(), "game".into()]).is_err());
        assert!(parse_role(&["--role".into()]).is_err());
        assert!(parse_role(&["--bogus".into()]).is_err());
    }
}
