//! `#[sensitivity]`, `#[owner]`, `#[requires_review]`, `#[deprecated_after]`: read the attribute, check what can be
//! checked while the crate compiles, and submit the mark.

use liaise_proc_macro::SynBuffer;
use maintenance_core::attrs::{self, as_attribute};
use maintenance_core::levels::LevelTable;
use maintenance_core::{Code, LiaiseCode};
use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};

use crate::emit::{mark, refuse_repeat, target, with_extra};

/// The item unchanged with the errors beside it: one problem shows once, and nothing else breaks for it.
fn refused(item: TokenStream, errs: SynBuffer) -> TokenStream {
    let errors = errs.into_compile_error();
    quote!(#errors #item)
}

pub(crate) fn sensitivity(args: TokenStream, item: TokenStream) -> TokenStream {
    let mut errs = SynBuffer::new();
    let read = attrs::read_sensitivity(&as_attribute(attrs::SENSITIVITY, args), &mut errs);
    let Some(target) = target(item.clone(), attrs::SENSITIVITY, &mut errs) else {
        return refused(item, errs);
    };
    refuse_repeat(&target.item, attrs::SENSITIVITY, &mut errs);
    let Some(read) = read else {
        return refused(item, errs);
    };
    let level = &read.level;
    let (what, check) = match &read.levels {
        // the shipped word: known here, refused here
        None => {
            let table = LevelTable::shipped();
            let Some(bit) = table.bit(level) else {
                errs.push_at_ctx(read.level_span, Code::UnknownLevel, format!("`{level}`; the levels are {}", table.listed()));
                return refused(item, errs);
            };
            (quote!(::maintenance_core::registry::Marked::Sensitivity { level: #level, rank: #bit }), TokenStream::new())
        }
        // a word the user declared: its flags are only known to the compiler, so the check is a constant
        Some(word) => {
            let flags = quote!(<#word as ::maintenance_core::levels::Levels>::FLAGS);
            let message = format!("{}: `{level}` is not a flag of `{}`", Code::UnknownLevel.render(), quote!(#word).to_string().replace(' ', ""));
            let check = quote_spanned! {read.level_span=>
                const _: () = if ::maintenance_core::levels::rank_of(#flags, #level).is_none() { ::core::panic!(#message) };
            };
            (quote!(::maintenance_core::registry::Marked::Sensitivity { level: #level, rank: ::maintenance_core::levels::rank_or_zero(#flags, #level) }), check)
        }
    };
    if !errs.is_empty() {
        return refused(item, errs);
    }
    let submit = mark(&target, what);
    with_extra(target, quote!(#check #submit))
}

pub(crate) fn owner(args: TokenStream, item: TokenStream) -> TokenStream {
    let mut errs = SynBuffer::new();
    let read = attrs::read_owner(&as_attribute(attrs::OWNER, args), &mut errs);
    simple_mark(item, attrs::OWNER, errs, read.map(|owner| quote!(::maintenance_core::registry::Marked::Owner { owner: #owner })))
}

pub(crate) fn requires_review(args: TokenStream, item: TokenStream) -> TokenStream {
    let mut errs = SynBuffer::new();
    let read = attrs::read_requires_review(&as_attribute(attrs::REQUIRES_REVIEW, args), &mut errs);
    simple_mark(item, attrs::REQUIRES_REVIEW, errs, read.map(|reason| quote!(::maintenance_core::registry::Marked::Review { reason: #reason })))
}

fn simple_mark(item: TokenStream, attr: &str, mut errs: SynBuffer, what: Option<TokenStream>) -> TokenStream {
    let Some(target) = target(item.clone(), attr, &mut errs) else {
        return refused(item, errs);
    };
    refuse_repeat(&target.item, attr, &mut errs);
    match what {
        Some(what) if errs.is_empty() => {
            let submit = mark(&target, what);
            with_extra(target, submit)
        }
        _ => refused(item, errs),
    }
}

pub(crate) fn deprecated_after(args: TokenStream, item: TokenStream) -> TokenStream {
    let mut errs = SynBuffer::new();
    let read = attrs::read_deprecated_after(&as_attribute(attrs::DEPRECATED_AFTER, args), &mut errs);
    let Some(mut target) = target(item.clone(), attrs::DEPRECATED_AFTER, &mut errs) else {
        return refused(item, errs);
    };
    refuse_repeat(&target.item, attrs::DEPRECATED_AFTER, &mut errs);
    if target.kind == "impl" {
        errs.push_at_ctx(target.name_span, Code::WrongItem, "#[deprecated_after] goes on what an impl holds, not on the impl");
    }
    let Some(read) = read else {
        return refused(item, errs);
    };
    let today = maintenance_core::dates::today_days();
    if today > read.days {
        errs.push_at_ctx(
            read.date_span,
            Code::DatePassed,
            format!("deprecated after {} ({}), today is {}", read.date, read.reason, maintenance_core::dates::show_days(today)),
        );
    }
    if !errs.is_empty() {
        return refused(item, errs);
    }
    let note = format!("deprecated after {}: {}", read.date, read.reason);
    let deprecated: syn::Attribute = syn::parse_quote!(#[deprecated(note = #note)]);
    if let Some(attrs) = item_attrs(&mut target.item) {
        attrs.insert(0, deprecated);
    }
    let (date, reason) = (&read.date, &read.reason);
    let submit = mark(&target, quote!(::maintenance_core::registry::Marked::DeprecatedAfter { date: #date, reason: #reason }));
    with_extra(target, submit)
}

fn item_attrs(item: &mut syn::Item) -> Option<&mut Vec<syn::Attribute>> {
    match item {
        syn::Item::Fn(i) => Some(&mut i.attrs),
        syn::Item::Struct(i) => Some(&mut i.attrs),
        syn::Item::Enum(i) => Some(&mut i.attrs),
        syn::Item::Union(i) => Some(&mut i.attrs),
        syn::Item::Trait(i) => Some(&mut i.attrs),
        syn::Item::Type(i) => Some(&mut i.attrs),
        syn::Item::Const(i) => Some(&mut i.attrs),
        syn::Item::Static(i) => Some(&mut i.attrs),
        syn::Item::Mod(i) => Some(&mut i.attrs),
        _ => None,
    }
}
