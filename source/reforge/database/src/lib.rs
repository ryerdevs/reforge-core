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
//! Dominios: `account` implementado (primer slice); world/social/economy/log
//! declarados como stubs doc hasta sus fases (F4/F5).

pub mod account;
pub mod player;
pub mod sha1;
pub mod wal;

/// F3/F4 (diferido): repositorios del dominio world — personajes, items,
/// quests, mascotas (schema `player`). Contrato pendiente de la fase world.
pub mod world {}

/// F4 (diferido): repositorios del dominio social — guildas, grupos,
/// messenger (schema `social`).
pub mod social {}

/// F4/F5 (diferido): repositorios del dominio economy — subasta, dinero,
/// historial de comercio (schema `economy`).
pub mod economy {}

/// F3 phase 2 / F5 (diferido): repositorios del dominio log — audit
/// append-only, particionado por fecha + retencion (schema `log`).
pub mod log {}
