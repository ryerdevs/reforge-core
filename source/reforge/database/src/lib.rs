//! F3 — data layer (ADR-0008): repositorios por dominio sobre PostgreSQL.
//!
//! - PostgreSQL-only (MariaDB es solo fuente de migracion — ADR-0001/0005).
//! - Acceso SOLO via repositorios (sin `direct-sql` fuera del crate).
//! - Driver: tokio-postgres 0.7 (verificado end-to-end en F2a; la decision
//!   con evidencia esta en ADR-0008).
//! - Un schema PG por dominio (account/player/common/log — migrados por G-PG);
//!   permisos por schema; RLS diferido (ADR-0008).
//! - Pipeline durable/volatile: durable = batch transaccional <=100ms;
//!   WAL local + mutation_id + replay idempotente (F3 phase 2, wal.rs).
//!
//! # PROTO_FROM_DB — estado actual (documentado, 2026-08-13)
//!
//! El flag `PROTO_FROM_DB` es del BASELINE C++ (el db binario lee
//! `mob_proto`/`item_proto` de la BD en vez de los txt). Aplica al C++
//! congelado via `mysql_proxy` (oracle de paridad) — NO al runtime Rust. El
//! equivalente moderno en el Rust: el channel carga el proto desde PG via
//! `MobRepo` (`npc.rs`) e `ItemRepo::load_proto_use_values` (`item.rs`) —
//! la BD es la fuente unica de proto (hot reload por diseño, plan §9).
//!
//! # Dominios
//!
//! - `account` — auth (login, mysql5 hash, lang/hwid).
//! - `world` — player/quest/affect/safebox/item/item_award (schema player).
//! - `social` — messenger + guildas (schema player; `social.rs`).
//! - `economy` — money log + guardas de oro (`economy.rs`; schemas log/player).
//! - `log` — audit append-only (F5; el DDL del audit vive en `wal::AUDIT_DDL`).

pub mod account;
pub mod affect;
pub mod common;
pub mod economy;
pub mod item;
pub mod land;
pub mod locale;
pub mod messenger;
pub mod npc;
pub mod player;
pub mod quest;
pub mod safebox;
pub mod sha1;
pub mod social;
pub mod wal;

/// F5 (diferido): repositorios del dominio log — audit append-only,
/// particionado por fecha + retencion (schema `log`). El DDL del audit
/// (`log.mutation_audit`) lo aplica el harness; el pipeline WAL ya escribe
/// ahi (wal.rs).
pub mod log {}
