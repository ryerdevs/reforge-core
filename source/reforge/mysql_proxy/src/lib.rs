//! `mysql_proxy` — adaptador temporal MySQL wire v10 → PostgreSQL (G-PG, ADR-0005,
//! spec §8.2.1c). Se borra en F6.
//!
//! El baseline C++ (`db`/`auth`) sigue linkando `libmariadb` y se conecta a este
//! proxy como si fuera MySQL (`127.0.0.1:3307`); el proxy traduce wire/SQL a
//! PostgreSQL. Fuente C++ intacta (oracle, ADR-0003); el único cambio de runtime
//! es conf.txt (spec §8.2.1c).
//!
//! - [`wire`] — codec del protocolo wire de MySQL v10, a mano y sin dependencia
//!   (HandshakeV10 + HandshakeResponse41 + auth `mysql_native_password` +
//!   COM_QUERY/COM_QUIT/COM_PING + OK/ERR/EOF/result set; sin prepared
//!   statements — `CStmt` tiene 0 call sites, `legacy-sql-compatibility.md` §2.1).
//! - [`translate`] — reescritura SQL MySQL→PG según la tabla §4 de
//!   `docs/reference/database/legacy-sql-compatibility.md` (esa tabla es la spec
//!   de los unit tests).
//! - [`session`] — sesión PG 1:1 por conexión MySQL (tokio-postgres),
//!   `search_path` por slot, catálogo de tablas (PK/columnas/identity) cacheado.
//! - [`config`] — parser TOML mínimo (ADR-0004; sin config-rs hasta F2).
//! - [`sha1`] — SHA-1 a mano (FIPS 180-1) para `mysql_native_password`; sin
//!   dependencia.
//! - [`server`] — loop de conexión (handshake → auth → bucle de comandos).
//! - [`debug`] — logging de diagnóstico (`--debug` / `MYSQL_PROXY_DEBUG=1`).

pub mod config;
pub mod debug;
pub mod server;
pub mod session;
pub mod sha1;
pub mod translate;
pub mod wire;

pub use config::Config;
pub use server::serve;
