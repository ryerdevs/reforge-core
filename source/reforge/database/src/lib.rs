//! F3 — data layer (ADR-0008): repositorios por dominio sobre PostgreSQL.
//!
//! - PostgreSQL-only (MariaDB es solo fuente de migracion — ADR-0001/0005).
//! - Acceso SOLO via repositorios (sin `direct-sql` fuera del crate).
//! - Driver: tokio-postgres 0.7 (verificado end-to-end en F2a; la decision
//!   con evidencia esta en ADR-0008).
//! - Un schema PG por dominio (account/player/common/log — migrados por G-PG);
//!   permisos por schema; RLS diferido (ADR-0008).
//! - Pipeline durable/volatile: durable = batch transaccional <=100ms;
//!   WAL local + mutation_id + replay idempotente diferidos a F3 phase 2.
//!
//! Dominios: `account` y `world` (player/quest/affect/safebox/item +
//! item_award) implementados; social (messenger) y economy/log declarados
//! como stubs doc hasta sus fases (F4/F5).

pub mod account;
pub mod affect;
pub mod item;
pub mod messenger;
pub mod player;
pub mod quest;
pub mod safebox;
pub mod sha1;
pub mod wal;

/// F3/F4 (diferido): resto del dominio social — guildas, grupos (schema
/// `social`). `messenger` ya esta implementado como modulo propio (F3).
pub mod social {}

/// F4/F5 (diferido): repositorios del dominio economy — subasta, dinero,
/// historial de comercio (schema `economy`). `safebox` (F3) vive en `player`.
pub mod economy {}

/// F3 phase 2 / F5 (diferido): repositorios del dominio log — audit
/// append-only, particionado por fecha + retencion (schema `log`).
pub mod log {}
