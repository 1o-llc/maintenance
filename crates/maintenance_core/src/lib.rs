//! The runtime half of `maintenance_macros`.
//!
//! The macros write marks and thresholds into the code; this crate is what those marks and thresholds mean once the
//! code runs, and what reads them back out:
//!
//! - [`codes`]: every code the set raises, in one `liaise_codes!` table, `[MAINT0000] message`.
//! - [`levels`]: sensitivity levels, a big-bad-bool word with a named flag per level ([`levels::Sensitivity`]).
//! - [`duration`] and [`config`]: thresholds in time_kit's words (`500ms`, `2s`, `3slots`), and `maintenance.toml`.
//! - [`timing`]: the clock a `#[timed_test]` runs under, and the report it appends to.
//! - [`registry`]: `.maintenance/registry.json`, every marked item in one file, and its reader.
//! - [`attrs`] (feature `attrs`): every attribute read one way, shared by the macros and the checker.
//! - [`check`] (feature `check`): `sensitivity check`, which reads the workspace's call graph through klepto.

#![forbid(unsafe_code)]

pub mod codes;
pub mod config;
pub mod dates;
pub mod duration;
pub mod fsio;
pub mod levels;
pub mod registry;
pub mod timing;

#[cfg(feature = "attrs")]
pub mod attrs;
#[cfg(feature = "check")]
pub mod check;

pub use codes::{Code, LiaiseCode};
pub use levels::{Levels, Sensitivity};

/// What the macros' expansions name. Not public API.
#[doc(hidden)]
pub mod __private {
    pub use inventory;
}
