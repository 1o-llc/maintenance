//! Attributes for keeping a codebase. Each has one job:
//!
//! - `#[sensitivity(secret)]` marks how sensitive an item is; `sensitivity check` reports every use of a more
//!   sensitive item from a less sensitive one.
//! - `sensitivity_levels!` declares a workspace's own levels, a big-bad-bool word with a flag per level.
//! - `#[timed_test(max = "2s")]` fails a test that takes longer than its threshold and reports every test's time.
//! - `#[owner("platform")]` records who keeps an item.
//! - `#[requires_review(reason = "...")]` records that a change to an item needs a second reader.
//! - `#[deprecated_after("2026-12-01", reason = "...")]` deprecates an item, and fails the build once the date passes.
//! - `#[mechanism("retry policy")]` names the one item that is the one place for a rule; a second claim is refused.
//! - `#[boundary(allowed = ["api"])]` on an inline module, or `boundary_here!(allowed = ["api"]);` inside a module or a
//!   crate root, names the crates allowed to use what is inside.
//! - `#[supersedes("crate::old")]` on a replacement: the check lists what still uses the old item.
//! - `#[reads_env("KEY")]` where a key is read: the check refuses a key no declaration file declares.
//! - `#[wire_writer]` on a function that writes bytes others read, and `#[golden(writer = path)]` on the test that
//!   pins them: the check refuses a writer with no golden.
//!
//! Use them through `maintenance`, which re-exports them: the expansions name `::maintenance`. The marks are recorded
//! twice from one reading: in the binary (`maintenance::registry::linked`) and, by `maintenance::check`, in
//! `.maintenance/registry.json`. Every refusal is a `[MAINT....]` code from the one table in
//! `maintenance_core::codes`.

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

/// `#[mechanism("name")]`: this item is the one place for the rule `name`.
#[proc_macro_attribute]
pub fn mechanism(args: TokenStream, item: TokenStream) -> TokenStream {
    marks::mechanism(args.into(), item.into()).into()
}

/// `#[boundary(allowed = ["crate_a", "crate_b"])]` on an inline `mod`: the crates, besides its own, that may use
/// what is inside. For a crate root or a module in its own file (where an attribute macro cannot go on stable Rust),
/// write `boundary_here!(allowed = [...]);` inside it.
#[proc_macro_attribute]
pub fn boundary(args: TokenStream, item: TokenStream) -> TokenStream {
    marks::boundary(args.into(), item.into()).into()
}

/// `boundary_here!(allowed = ["crate_a"]);` inside a module or at a crate root: `#[boundary]` for the module it is in.
#[proc_macro]
pub fn boundary_here(input: TokenStream) -> TokenStream {
    marks::boundary_here(input.into()).into()
}

/// `#[supersedes("path::to::old")]` (or `#[supersedes(path::to::old)]`) on the item that replaces `old`.
#[proc_macro_attribute]
pub fn supersedes(args: TokenStream, item: TokenStream) -> TokenStream {
    marks::supersedes(args.into(), item.into()).into()
}

/// `#[reads_env("KEY", ...)]` on the item that reads the environment keys.
#[proc_macro_attribute]
pub fn reads_env(args: TokenStream, item: TokenStream) -> TokenStream {
    marks::reads_env(args.into(), item.into()).into()
}

/// `#[golden(writer = path::to::writer)]` on a test that pins `writer`'s bytes. The path is checked by the compiler.
#[proc_macro_attribute]
pub fn golden(args: TokenStream, item: TokenStream) -> TokenStream {
    marks::golden(args.into(), item.into()).into()
}

/// `#[wire_writer]` on a function whose bytes something else reads: it needs a `#[golden]` test.
#[proc_macro_attribute]
pub fn wire_writer(args: TokenStream, item: TokenStream) -> TokenStream {
    marks::wire_writer(args.into(), item.into()).into()
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
