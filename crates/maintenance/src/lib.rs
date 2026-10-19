//! Keeping a codebase: attributes that say what an item is, and the checks that hold the workspace to it.
//!
//! - The attributes ([`sensitivity`], [`timed_test`], [`owner`], [`requires_review`], [`deprecated_after`],
//!   [`mechanism`], [`boundary`] / [`boundary_here!`], [`supersedes`], [`reads_env`], [`golden`], [`wire_writer`],
//!   [`sensitivity_levels!`]) come from `maintenance_macros`, re-exported here: depend on this crate alone.
//! - [`registry`]: `.maintenance/registry.json`, every marked item in one file, and its reader [`registry::read`].
//! - [`timed`]: the clock a `#[timed_test]` runs under, the run's report it appends to, and [`timed::report`].
//! - [`check`] (feature `check`): the workspace checks, read through klepto, each returning a [`check::Report`].
//! - [`runner`] (feature `runner`): the checks and the timing report declared as xccute lines of the `maintenance`
//!   program, which the bin (feature `bin`) runs.
//! - [`codes`], [`levels`], [`config`], [`duration`], [`dates`]: `maintenance_core`, what the macros and the runtime
//!   both read.

#![forbid(unsafe_code)]

extern crate self as maintenance;

pub use maintenance_core::{codes, config, dates, duration, levels, Code, LiaiseCode, Levels, Sensitivity};
pub use maintenance_macros::{
    boundary, boundary_here, deprecated_after, golden, mechanism, owner, reads_env, requires_review, sensitivity, sensitivity_levels, supersedes,
    timed_test, wire_writer,
};

pub mod fsio;
pub mod registry;
pub mod timed;

#[cfg(feature = "check")]
pub mod check;
#[cfg(feature = "runner")]
pub mod runner;

/// What the macros' expansions name. Not public API.
#[doc(hidden)]
pub mod __private {
    pub use inventory;
}
