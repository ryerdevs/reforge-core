//! # Metin2 Quest DSL — core engine
//!
//! Own declarative quest DSL (spec: `docs/reference/quests/quest-dsl.md`).
//! Replaces the legacy Lua 5.0 quest runtime (no scripting — composition of
//! typed, known triggers/conditions/actions only).
//!
//! ## Open decisions (§11 of the spec) — resolved 2026-08-13
//!
//! | Decision | Resolution |
//! |---|---|
//! | `between a, b` | **Native syntax** — `pc.level between 15, 39`; parsed to a range condition. Friendlier for quest designers (spec §4). |
//! | `if` depth | **1 level + else** — branches only over captured results (`as`) and simple conditions; no nesting, no loops (spec §10). |
//! | `select` capture | **`as <name>`** — `-> select(@a, @b) as choice`; the branch compares the capture. |
//! | Locale keys | **`@key`** — the `@` prefix marks a locale key; families carry their own key index (spec §6). |
//! | Extension | **`.quest`** — same as legacy, eases diff during migration (spec §2). |
//! | `timer` trigger | **Alias only** — recognized by the parser (legacy compatibility) but the recommended pattern is `on login with get_time() >= get_qf(duration)` (spec §11.6); no special runtime state. |
//!
//! ## Scope
//!
//! This crate is the CORE: lexer/parser (typed catalog validation), AST,
//! family expansion and a debug renderer. The RUNTIME engine (state machine +
//! `wait()` scheduler) and the qc→DSL converter are separate future slices
//! (spec §12). Special event managers (`oxevent`, `christmas_*`) are Rust
//! server modules, NOT DSL (spec §8).
//!
//! ## Grammar (summary)
//!
//! ```text
//! quest <name>                        | quest <name> = <base>(<param>: <value>, ...)
//!   import <file>
//!   block <name>(<param>: <type>, ...)
//!     -> <action>(<args>)
//!   state <name>
//!     on <trigger>[ , <trigger>...] [with <expr>]
//!       -> <action>(<args>) [as <capture>]
//!       if <expr> | else
//!       use <name>(<args>)
//! ```
//!
//! Indentation-significant (2 spaces), `#` comments. Every trigger,
//! condition and action is known to the parser (typed catalog) — an unknown
//! name is a load error with file:line:column.

pub mod ast;
pub mod convert;
pub mod family;
pub mod parser;
pub mod render;

pub use ast::*;
pub use family::{
    QuestSimilarity, SimilarGroup, detect_similar_groups, expand_families, quest_similarity,
};
pub use parser::{ParseError, parse};

/// Result of a parsed quest file (the unit the runtime will consume).
pub type QuestFile = Vec<ast::Quest>;
