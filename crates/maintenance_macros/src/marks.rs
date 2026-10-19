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
            (quote!(::maintenance::registry::Marked::Sensitivity { level: #level, rank: #bit }), TokenStream::new())
        }
        // a word the user declared: its flags are only known to the compiler, so the check is a constant
        Some(word) => {
            let flags = quote!(<#word as ::maintenance::levels::Levels>::FLAGS);
            let message = format!("{}: `{level}` is not a flag of `{}`", Code::UnknownLevel.render(), quote!(#word).to_string().replace(' ', ""));
            let check = quote_spanned! {read.level_span=>
                const _: () = if ::maintenance::levels::rank_of(#flags, #level).is_none() { ::core::panic!(#message) };
            };
            (quote!(::maintenance::registry::Marked::Sensitivity { level: #level, rank: ::maintenance::levels::rank_or_zero(#flags, #level) }), check)
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
    simple_mark(item, attrs::OWNER, errs, read.map(|owner| quote!(::maintenance::registry::Marked::Owner { owner: #owner })))
}

pub(crate) fn requires_review(args: TokenStream, item: TokenStream) -> TokenStream {
    let mut errs = SynBuffer::new();
    let read = attrs::read_requires_review(&as_attribute(attrs::REQUIRES_REVIEW, args), &mut errs);
    simple_mark(item, attrs::REQUIRES_REVIEW, errs, read.map(|reason| quote!(::maintenance::registry::Marked::Review { reason: #reason })))
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
    let submit = mark(&target, quote!(::maintenance::registry::Marked::DeprecatedAfter { date: #date, reason: #reason }));
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

pub(crate) fn mechanism(args: TokenStream, item: TokenStream) -> TokenStream {
    let mut errs = SynBuffer::new();
    let read = attrs::read_mechanism(&as_attribute(attrs::MECHANISM, args), &mut errs);
    simple_mark(item, attrs::MECHANISM, errs, read.map(|name| quote!(::maintenance::registry::Marked::Mechanism { name: #name })))
}

fn boundary_what(allowed: &[String]) -> TokenStream {
    quote!(::maintenance::registry::Marked::Boundary { allowed: &[#(#allowed),*] })
}

pub(crate) fn boundary(args: TokenStream, item: TokenStream) -> TokenStream {
    let mut errs = SynBuffer::new();
    let read = attrs::read_boundary(&as_attribute(attrs::BOUNDARY, args), &mut errs);
    if let Ok(syn::Item::Mod(_)) = syn::parse2::<syn::Item>(item.clone()) {
    } else {
        errs.push_spanned_ctx(&item, Code::WrongItem, "#[boundary] goes on an inline mod; inside a module or a crate root, write boundary_here!(allowed = [...]);");
        return refused(item, errs);
    }
    simple_mark(item, attrs::BOUNDARY, errs, read.map(|allowed| boundary_what(&allowed)))
}

pub(crate) fn boundary_here(input: TokenStream) -> TokenStream {
    let mut errs = SynBuffer::new();
    let at = input.clone();
    let Some(allowed) = attrs::read_boundary(&as_attribute(attrs::BOUNDARY, input), &mut errs) else {
        return errs.into_compile_error();
    };
    let what = boundary_what(&allowed);
    let line = quote_spanned!(syn::spanned::Spanned::span(&at)=> ::core::line!());
    quote! {
        ::maintenance::__private::inventory::submit! {
            ::maintenance::registry::Mark { module: ::core::module_path!(), name: "", kind: "mod", file: ::core::file!(), line: #line, what: #what }
        }
    }
}

pub(crate) fn supersedes(args: TokenStream, item: TokenStream) -> TokenStream {
    let mut errs = SynBuffer::new();
    let read = attrs::read_supersedes(&as_attribute(attrs::SUPERSEDES, args), &mut errs);
    simple_mark(item, attrs::SUPERSEDES, errs, read.map(|old| {
        let old = attrs::path_text(&old);
        quote!(::maintenance::registry::Marked::Supersedes { old: #old })
    }))
}

pub(crate) fn reads_env(args: TokenStream, item: TokenStream) -> TokenStream {
    let mut errs = SynBuffer::new();
    let read = attrs::read_reads_env(&as_attribute(attrs::READS_ENV, args), &mut errs);
    simple_mark(item, attrs::READS_ENV, errs, read.map(|keys| quote!(::maintenance::registry::Marked::ReadsEnv { keys: &[#(#keys),*] })))
}

pub(crate) fn golden(args: TokenStream, item: TokenStream) -> TokenStream {
    let mut errs = SynBuffer::new();
    let read = attrs::read_golden(&as_attribute(attrs::GOLDEN, args), &mut errs);
    let Some(target) = target(item.clone(), attrs::GOLDEN, &mut errs) else {
        return refused(item, errs);
    };
    refuse_repeat(&target.item, attrs::GOLDEN, &mut errs);
    match read {
        Some(writer) if errs.is_empty() => {
            let text = attrs::path_text(&writer);
            let submit = mark(&target, quote!(::maintenance::registry::Marked::Golden { writer: #text }));
            // the writer is named as code, so a writer that does not exist is the compiler's error, on the path
            with_extra(target, quote!(#[allow(unused_imports)] use #writer as _; #submit))
        }
        _ => refused(item, errs),
    }
}

pub(crate) fn wire_writer(args: TokenStream, item: TokenStream) -> TokenStream {
    let mut errs = SynBuffer::new();
    let read = attrs::read_wire_writer(&as_attribute(attrs::WIRE_WRITER, args), &mut errs);
    simple_mark(item, attrs::WIRE_WRITER, errs, read.map(|()| quote!(::maintenance::registry::Marked::WireWriter)))
}
