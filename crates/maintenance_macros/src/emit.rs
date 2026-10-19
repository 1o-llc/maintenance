//! Where a mark goes in the item it is on, and the one shape every mark is submitted in.

use maintenance_core::Code;
use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned, ToTokens};
use syn::spanned::Spanned;

/// An item an attribute is on, read.
pub(crate) struct Target {
    pub item: syn::Item,
    pub name: String,
    pub kind: &'static str,
    pub name_span: Span,
}

/// Read the item. Only items a mark can describe are taken: a `use`, a macro call or an `extern` block is refused.
pub(crate) fn target(item: TokenStream, attr: &str, errs: &mut liaise_proc_macro::SynBuffer) -> Option<Target> {
    let item: syn::Item = match syn::parse2(item.clone()) {
        Ok(item) => item,
        Err(_) => {
            errs.push_spanned_ctx(item, Code::WrongItem, format!("#[{attr}] goes on an item with a body or a definition"));
            return None;
        }
    };
    let (name, kind, span) = match &item {
        syn::Item::Fn(f) => (f.sig.ident.to_string(), "fn", f.sig.ident.span()),
        syn::Item::Struct(s) => (s.ident.to_string(), "struct", s.ident.span()),
        syn::Item::Enum(e) => (e.ident.to_string(), "enum", e.ident.span()),
        syn::Item::Union(u) => (u.ident.to_string(), "union", u.ident.span()),
        syn::Item::Trait(t) => (t.ident.to_string(), "trait", t.ident.span()),
        syn::Item::Type(t) => (t.ident.to_string(), "type", t.ident.span()),
        syn::Item::Const(c) => (c.ident.to_string(), "const", c.ident.span()),
        syn::Item::Static(s) => (s.ident.to_string(), "static", s.ident.span()),
        syn::Item::Mod(m) => (m.ident.to_string(), "mod", m.ident.span()),
        syn::Item::Impl(i) => (i.self_ty.to_token_stream().to_string().replace(' ', ""), "impl", i.self_ty.span()),
        other => {
            errs.push_spanned_ctx(other, Code::WrongItem, format!("#[{attr}] goes on a fn, struct, enum, union, trait, type, const, static, mod or impl"));
            return None;
        }
    };
    Some(Target { item, name, kind, name_span: span })
}

/// A second attribute of the same name further down the item is refused here, where the first one sees it.
pub(crate) fn refuse_repeat(item: &syn::Item, attr: &str, errs: &mut liaise_proc_macro::SynBuffer) {
    let attrs: &[syn::Attribute] = match item {
        syn::Item::Fn(i) => &i.attrs,
        syn::Item::Struct(i) => &i.attrs,
        syn::Item::Enum(i) => &i.attrs,
        syn::Item::Union(i) => &i.attrs,
        syn::Item::Trait(i) => &i.attrs,
        syn::Item::Type(i) => &i.attrs,
        syn::Item::Const(i) => &i.attrs,
        syn::Item::Static(i) => &i.attrs,
        syn::Item::Mod(i) => &i.attrs,
        syn::Item::Impl(i) => &i.attrs,
        _ => &[],
    };
    for again in attrs.iter().filter(|a| maintenance_core::attrs::is_named(a, attr)) {
        errs.push_spanned_ctx(again.path(), Code::Repeated, format!("#[{attr}]"));
    }
}

/// The item with `extra` (marks, checks) beside it: as the first statements of a fn's body, so a mark on a method
/// stays inside its impl, and after any other item.
pub(crate) fn with_extra(target: Target, extra: TokenStream) -> TokenStream {
    match target.item {
        syn::Item::Fn(mut f) => {
            let block = &f.block;
            let stmts = &block.stmts;
            f.block = syn::parse_quote_spanned!(block.span()=> { #extra #(#stmts)* });
            f.into_token_stream()
        }
        item => quote!(#item #extra),
    }
}

/// `inventory::submit!` of one `maintenance_core::registry::Mark`.
pub(crate) fn mark(target: &Target, what: TokenStream) -> TokenStream {
    let (name, kind) = (&target.name, target.kind);
    let line = quote_spanned!(target.name_span=> ::core::line!());
    quote! {
        ::maintenance_core::__private::inventory::submit! {
            ::maintenance_core::registry::Mark {
                module: ::core::module_path!(),
                name: #name,
                kind: #kind,
                file: ::core::file!(),
                line: #line,
                what: #what,
            }
        }
    }
}
