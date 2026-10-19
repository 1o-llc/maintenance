//! What `maintenance` and `maintenance_macros` both read, so a rule is written once for the compile and the run:
//!
//! - [`codes`]: every code the set raises, in one `liaise_codes!` table, `[MAINT0000] message`.
//! - [`levels`]: sensitivity levels, a big-bad-bool word with a named flag per level ([`levels::Sensitivity`]), and
//!   the reader of a `sensitivity_levels!` declaration.
//! - [`duration`] and [`config`]: thresholds in time_kit's words (`500ms`, `2s`, `3slots`), and `maintenance.toml`.
//! - [`dates`]: `YYYY-MM-DD`, for `#[deprecated_after]`.
//! - [`attrs`] (feature `attrs`): every attribute read one way, by the macros while a crate compiles and by the
//!   checks from the source.
//!
//! A crate using the attributes depends on `maintenance`, which re-exports this crate and the macros.

#![forbid(unsafe_code)]

pub mod codes;
pub mod config;
pub mod dates;
pub mod duration;
pub mod levels;

#[cfg(feature = "attrs")]
pub mod attrs;

pub use codes::{Code, LiaiseCode};
pub use levels::{Levels, Sensitivity};
