//! Attributes for keeping a codebase. Each has one job:
//!
//! - `#[sensitivity(secret)]` marks how sensitive an item is; `sensitivity check` reports every use of a more
//!   sensitive item from a less sensitive one.
//! - `sensitivity_levels!` declares a workspace's own levels, a big-bad-bool word with a flag per level.
//! - `#[timed_test(max = "2s")]` fails a test that takes longer than its threshold and reports every test's time.
//! - `#[owner("platform")]` records who keeps an item.
//! - `#[requires_review(reason = "...")]` records that a change to an item needs a second reader.
//! - `#[deprecated_after("2026-12-01", reason = "...")]` deprecates an item, and fails the build once the date passes.
//!
//! The marks are recorded twice from one reading: in the binary (`maintenance_core::registry::linked`) and, by
//! `sensitivity registry`, in `.maintenance/registry.json`. Every refusal is a `[MAINT....]` code from the one table
//! in `maintenance_core::codes`. A crate using these depends on `maintenance_core` too: the expansions name it.

#![forbid(unsafe_code)]

use proc_macro::TokenStream;

mod emit;
mod levels;
mod marks;
mod timed;

/// `#[sensitivity(level)]` or `#[sensitivity(level, levels = path::to::Word)]` on a fn, struct, enum, union, trait,
/// type, const, static, inline mod or impl. The level is a flag of the shipped `Sensitivity` word, or of the named
/// word (one `sensitivity_levels!` declared).
#[proc_macro_attribute]
pub fn sensitivity(args: TokenStream, item: TokenStream) -> TokenStream {
    marks::sensitivity(args.into(), item.into()).into()
}

/// `#[owner("team")]`: who keeps the item.
#[proc_macro_attribute]
pub fn owner(args: TokenStream, item: TokenStream) -> TokenStream {
    marks::owner(args.into(), item.into()).into()
}

/// `#[requires_review(reason = "...")]`: a change to the item needs a second reader.
#[proc_macro_attribute]
pub fn requires_review(args: TokenStream, item: TokenStream) -> TokenStream {
    marks::requires_review(args.into(), item.into()).into()
}

/// `#[deprecated_after("YYYY-MM-DD", reason = "...")]`: `#[deprecated]` until the date, a compile error after it.
#[proc_macro_attribute]
pub fn deprecated_after(args: TokenStream, item: TokenStream) -> TokenStream {
    marks::deprecated_after(args.into(), item.into()).into()
}

/// `#[timed_test]`, `#[timed_test(max = "2s")]`, `#[timed_test(class = "integration")]` on a test fn. Above
/// `#[tokio::test]` (or any runtime's `test`) for an async test.
#[proc_macro_attribute]
pub fn timed_test(args: TokenStream, item: TokenStream) -> TokenStream {
    timed::timed_test(args.into(), item.into()).into()
}

/// `sensitivity_levels! { pub TradeLevels { public = 0, internal = 1, trade_secret = 2 } }`: a level word, lowest
/// first by bit. The crate declaring one depends on `big-bad-bool`: the word is a `#[bitpack]` word.
#[proc_macro]
pub fn sensitivity_levels(input: TokenStream) -> TokenStream {
    levels::sensitivity_levels(input.into()).into()
}
