//! Every attribute this set takes, read one way: the macros read them here while a crate compiles, and the
//! checker reads them here from the source, so the two never disagree about what an attribute says.
//!
//! Keys go through `simple_impl_attr_kit`'s grammar and schemas ([`vocabulary`]); a positional string (`#[owner("x")]`,
//! the date of `#[deprecated_after]`) is read as a literal, as simple_impl's `#[requires("other")]` is. Every refusal
//! is a `[MAINT....]` code at the token it is about; a schema's own message (an unknown key, a wrong kind) follows the
//! code as its context.

use liaise_proc_macro::{err_at_ctx, err_spanned, err_spanned_ctx, SynBuffer};
use proc_macro2::{Span, TokenStream};
use simple_impl_attr_kit::{AttrBag, AttrValue};
use syn::spanned::Spanned;

use crate::codes::Code;

/// The attribute names, by what they do.
pub const SENSITIVITY: &str = "sensitivity";
pub const OWNER: &str = "owner";
pub const REQUIRES_REVIEW: &str = "requires_review";
pub const DEPRECATED_AFTER: &str = "deprecated_after";
pub const TIMED_TEST: &str = "timed_test";

/// The schema each keyed attribute validates against, and the line that documents each key.
pub mod vocabulary {
    use simple_impl_attr_kit::{AttrExpected as K, AttrSchema};

    /// `#[sensitivity(level, levels = Word)]`: the level is the one bare key the schema does not name.
    pub fn sensitivity() -> AttrSchema {
        AttrSchema::new()
            .optional("levels", K::Any)
            .doc("levels", "the level word the level is a flag of, written as a path (levels = crate::TradeLevels) or as a string; the shipped Sensitivity when absent")
            .allow_unknown()
    }

    /// `#[timed_test(max = "2s")]`, `#[timed_test(class = "integration")]`, `#[timed_test]`.
    pub fn timed_test() -> AttrSchema {
        AttrSchema::new()
            .optional("max", K::String)
            .doc("max", "the longest the test may take: 500ms, 2s, 1m, 3slots, 1m30s")
            .optional("class", K::String)
            .doc("class", "a class from maintenance.toml's [timed_test.classes] (or a shipped one: unit, integration, slow) whose threshold the test gets")
            .conflicts("max", "class")
    }

    /// `#[requires_review(reason = "...")]`.
    pub fn requires_review() -> AttrSchema {
        AttrSchema::new().optional("reason", K::String).doc("reason", "why a change to this item needs a second reader")
    }

    /// The keys after `#[deprecated_after("YYYY-MM-DD", ...)]`'s date.
    pub fn deprecated_after() -> AttrSchema {
        AttrSchema::new().optional("reason", K::String).doc("reason", "why it goes, and what to use instead")
    }
}

/// `#[sensitivity(..)]`, read.
#[derive(Debug, Clone)]
pub struct SensitivityAttr {
    pub level: String,
    pub level_span: Span,
    /// The level word as written (a path), when one is named.
    pub levels: Option<syn::Path>,
}

/// `#[timed_test(..)]`, read.
#[derive(Debug, Clone, Default)]
pub struct TimedTestAttr {
    pub max: Option<(String, Span)>,
    pub class: Option<(String, Span)>,
}

/// `#[deprecated_after(..)]`, read.
#[derive(Debug, Clone)]
pub struct DeprecatedAfterAttr {
    pub date: String,
    pub days: i64,
    pub date_span: Span,
    pub reason: String,
}

/// An attribute macro's arguments as the attribute they were written in: `#[name(args)]`.
pub fn as_attribute(name: &str, args: TokenStream) -> syn::Attribute {
    let ident = syn::Ident::new(name, Span::call_site());
    if args.is_empty() {
        syn::parse_quote!(#[#ident])
    } else {
        syn::parse_quote!(#[#ident(#args)])
    }
}

/// Whether `attr` is the attribute `name`, written bare or by a path ending in it (`maintenance_macros::owner`).
pub fn is_named(attr: &syn::Attribute, name: &str) -> bool {
    attr.path().segments.last().is_some_and(|s| s.ident == name)
}

/// The same attribute under its bare name, so the grammar reads `#[maintenance_macros::owner(..)]` as `#[owner(..)]`.
fn bare(attr: &syn::Attribute, name: &str) -> syn::Attribute {
    let mut attr = attr.clone();
    attr.meta = match attr.meta {
        syn::Meta::Path(_) => syn::Meta::Path(syn::Ident::new(name, attr.path().span()).into()),
        syn::Meta::List(mut list) => {
            list.path = syn::Ident::new(name, list.path.span()).into();
            syn::Meta::List(list)
        }
        syn::Meta::NameValue(mut nv) => {
            nv.path = syn::Ident::new(name, nv.path.span()).into();
            syn::Meta::NameValue(nv)
        }
    };
    attr
}

/// Each of an error's messages under `code`, at its own place.
fn coded(code: Code, e: syn::Error) -> syn::Error {
    let mut errs = SynBuffer::new();
    for one in e {
        errs.push(err_at_ctx(one.span(), code, one));
    }
    errs.finish().err().unwrap_or_else(|| err_at_ctx(Span::call_site(), code, "no message"))
}

fn keyed(attr: &syn::Attribute, name: &str, schema: simple_impl_attr_kit::AttrSchema, errs: &mut SynBuffer) -> Option<AttrBag> {
    errs.absorb(AttrBag::from_attr(&bare(attr, name), name).and_then(|bag| schema.validate(&bag)).map_err(|e| coded(Code::AttrUnreadable, e)))
}

/// Read `#[sensitivity(level)]` / `#[sensitivity(level, levels = Word)]`.
pub fn read_sensitivity(attr: &syn::Attribute, errs: &mut SynBuffer) -> Option<SensitivityAttr> {
    let bag = keyed(attr, SENSITIVITY, vocabulary::sensitivity(), errs)?;
    let mut levels = None;
    if let Some(value) = bag.get("levels") {
        let span = bag.span_of("levels").unwrap_or_else(Span::call_site);
        let path = match (value, bag.written_tokens("levels")) {
            (AttrValue::Str(text), _) => syn::parse_str::<syn::Path>(text).map_err(|e| err_at_ctx(span, Code::AttrUnreadable, format!("levels: {e}"))),
            // a path written as code: kept as written, so an error inside it lands where it was written
            (_, Some(tokens)) => syn::parse2::<syn::Path>(tokens.clone()).map_err(|_| err_spanned_ctx(tokens, Code::AttrUnreadable, "levels names a level word: a path")),
            (_, None) => Err(err_at_ctx(span, Code::AttrUnreadable, "levels names a level word: levels = path::to::Word")),
        };
        levels = Some(errs.absorb(path)?);
    }
    let named: Vec<&String> = bag.keys().filter(|k| *k != "levels").collect();
    match named.as_slice() {
        [] => {
            errs.push_at(attr.span(), Code::LevelMissing);
            None
        }
        [level] => {
            let span = bag.span_of(level).unwrap_or_else(|| attr.span());
            if bag.get(level) != Some(&AttrValue::Bool(true)) || bag.written_tokens(level).is_some() {
                errs.push_at_ctx(span, Code::AttrUnreadable, format!("a level is a bare name: #[sensitivity({level})]"));
                return None;
            }
            Some(SensitivityAttr { level: (*level).clone(), level_span: span, levels })
        }
        [_, second, ..] => {
            let names: Vec<&str> = named.iter().map(|s| s.as_str()).collect();
            errs.push_at_ctx(bag.span_of(second).unwrap_or_else(|| attr.span()), Code::TwoLevels, names.join(", "));
            None
        }
    }
}

/// Read `#[timed_test]`, `#[timed_test(max = "2s")]`, `#[timed_test(class = "integration")]`.
pub fn read_timed_test(attr: &syn::Attribute, errs: &mut SynBuffer) -> Option<TimedTestAttr> {
    let bag = keyed(attr, TIMED_TEST, vocabulary::timed_test(), errs)?;
    let read = |key: &str| bag.get(key).and_then(AttrValue::as_str).map(|s| (s.to_owned(), bag.span_of(key).unwrap_or_else(Span::call_site)));
    Some(TimedTestAttr { max: read("max"), class: read("class") })
}

/// Read `#[requires_review(reason = "...")]`: the reason.
pub fn read_requires_review(attr: &syn::Attribute, errs: &mut SynBuffer) -> Option<String> {
    let bag = keyed(attr, REQUIRES_REVIEW, vocabulary::requires_review(), errs)?;
    match bag.get("reason").and_then(AttrValue::as_str).map(str::trim) {
        Some(reason) if !reason.is_empty() => Some(reason.to_owned()),
        _ => {
            errs.push_at(bag.span_of("reason").unwrap_or_else(|| attr.span()), Code::ReviewReasonMissing);
            None
        }
    }
}

/// Read `#[owner("team")]`: the owner.
pub fn read_owner(attr: &syn::Attribute, errs: &mut SynBuffer) -> Option<String> {
    let owner = match &attr.meta {
        syn::Meta::List(list) => syn::parse2::<syn::LitStr>(list.tokens.clone()).ok(),
        _ => None,
    };
    match owner {
        Some(lit) if !lit.value().trim().is_empty() => Some(lit.value().trim().to_owned()),
        Some(lit) => {
            errs.push_spanned(&lit, Code::OwnerMissing);
            None
        }
        None => {
            errs.push_spanned(attr, Code::OwnerMissing);
            None
        }
    }
}

/// Read `#[deprecated_after("YYYY-MM-DD", reason = "...")]`.
pub fn read_deprecated_after(attr: &syn::Attribute, errs: &mut SynBuffer) -> Option<DeprecatedAfterAttr> {
    let syn::Meta::List(list) = &attr.meta else {
        errs.push_spanned_ctx(attr, Code::BadDate, "#[deprecated_after(\"YYYY-MM-DD\", reason = \"...\")]");
        return None;
    };
    let split = |input: syn::parse::ParseStream<'_>| -> syn::Result<(syn::LitStr, TokenStream)> {
        let date: syn::LitStr = input.parse()?;
        if !input.is_empty() {
            input.parse::<syn::Token![,]>()?;
        }
        Ok((date, input.parse()?))
    };
    let (date, rest) = match syn::parse::Parser::parse2(split, list.tokens.clone()) {
        Ok(read) => read,
        Err(e) => {
            errs.push_at_ctx(e.span(), Code::BadDate, "the date comes first, as a string: #[deprecated_after(\"2026-12-01\", reason = \"...\")]");
            return None;
        }
    };
    let days = errs.absorb(crate::dates::parse_days(&date.value()).map_err(|e| err_spanned_ctx(&date, Code::BadDate, e)));
    let keys = as_attribute(DEPRECATED_AFTER, rest);
    let bag = keyed(&keys, DEPRECATED_AFTER, vocabulary::deprecated_after(), errs)?;
    let reason = match bag.get("reason").and_then(AttrValue::as_str).map(str::trim) {
        Some(reason) if !reason.is_empty() => Some(reason.to_owned()),
        _ => {
            errs.push(err_spanned(&date, Code::ReasonMissing));
            None
        }
    };
    Some(DeprecatedAfterAttr { date: date.value(), days: days?, date_span: date.span(), reason: reason? })
}

/// Every mark one item carries, read from its attributes; each attribute that does not read is in `errs`.
#[derive(Debug, Clone, Default)]
pub struct Marks {
    pub sensitivity: Option<SensitivityAttr>,
    pub owner: Option<String>,
    pub review: Option<String>,
    pub deprecated_after: Option<DeprecatedAfterAttr>,
}

impl Marks {
    pub fn is_empty(&self) -> bool {
        self.sensitivity.is_none() && self.owner.is_none() && self.review.is_none() && self.deprecated_after.is_none()
    }
}

/// The marks in an item's attributes. One of a kind per item; a second is [`Code::Repeated`].
pub fn read_marks(attrs: &[syn::Attribute], errs: &mut SynBuffer) -> Marks {
    let mut marks = Marks::default();
    let mut seen: Vec<&str> = Vec::new();
    for attr in attrs {
        let Some(name) = [SENSITIVITY, OWNER, REQUIRES_REVIEW, DEPRECATED_AFTER].into_iter().find(|n| is_named(attr, n)) else {
            continue;
        };
        if seen.contains(&name) {
            errs.push_spanned_ctx(attr.path(), Code::Repeated, format!("#[{name}]"));
            continue;
        }
        seen.push(name);
        match name {
            SENSITIVITY => marks.sensitivity = read_sensitivity(attr, errs),
            OWNER => marks.owner = read_owner(attr, errs),
            REQUIRES_REVIEW => marks.review = read_requires_review(attr, errs),
            _ => marks.deprecated_after = read_deprecated_after(attr, errs),
        }
    }
    marks
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse::Parser;

    fn attrs(src: &str) -> Vec<syn::Attribute> {
        syn::Attribute::parse_outer.parse_str(src).unwrap()
    }

    fn refusals(src: &str) -> Vec<String> {
        let mut errs = SynBuffer::new();
        let _ = read_marks(&attrs(src), &mut errs);
        errs.finish().map_or_else(|e| e.into_iter().map(|e| e.to_string()).collect(), |()| Vec::new())
    }

    #[test]
    fn every_mark_reads_bare_or_by_path() {
        let mut errs = SynBuffer::new();
        let m = read_marks(
            &attrs(
                "#[maintenance_macros::sensitivity(secret)] #[owner(\"platform\")] #[requires_review(reason = \"keys\")] \
                 #[deprecated_after(\"2026-12-01\", reason = \"use v2\")] #[doc = \"x\"]",
            ),
            &mut errs,
        );
        assert!(errs.is_empty());
        assert_eq!(m.sensitivity.as_ref().map(|s| s.level.as_str()), Some("secret"));
        assert!(m.sensitivity.unwrap().levels.is_none());
        assert_eq!(m.owner.as_deref(), Some("platform"));
        assert_eq!(m.review.as_deref(), Some("keys"));
        let d = m.deprecated_after.unwrap();
        assert_eq!((d.date.as_str(), d.days, d.reason.as_str()), ("2026-12-01", 20_788, "use v2"));
    }

    #[test]
    fn a_level_word_is_written_as_code_or_as_a_string() {
        let mut errs = SynBuffer::new();
        let path = |src| read_sensitivity(&attrs(src)[0], &mut SynBuffer::new()).unwrap().levels.map(|p| quote::quote!(#p).to_string());
        assert_eq!(path("#[sensitivity(trade, levels = crate::Trade)]").as_deref(), Some("crate :: Trade"));
        assert_eq!(path("#[sensitivity(levels = \"crate::Trade\", trade)]").as_deref(), Some("crate :: Trade"));
        assert!(read_sensitivity(&attrs("#[sensitivity(trade, levels = 1 + 2)]")[0], &mut errs).is_none());
        assert_eq!(errs.finish().unwrap_err().to_string(), "[MAINT0001] the attribute does not read: levels names a level word: a path");
    }

    #[test]
    fn every_refusal_is_coded() {
        assert_eq!(refusals("#[sensitivity]"), ["[MAINT1001] #[sensitivity(level)] names one level: #[sensitivity(secret)]"]);
        assert_eq!(refusals("#[sensitivity(secret, public)]"), ["[MAINT1003] #[sensitivity] names one level; an item has one: public, secret"]);
        assert_eq!(refusals("#[sensitivity(secret = 1)]"), ["[MAINT0001] the attribute does not read: a level is a bare name: #[sensitivity(secret)]"]);
        assert_eq!(refusals("#[owner]"), ["[MAINT4001] #[owner] names its owner as a string: #[owner(\"platform\")]"]);
        assert_eq!(refusals("#[owner(\" \")]"), ["[MAINT4001] #[owner] names its owner as a string: #[owner(\"platform\")]"]);
        assert_eq!(refusals("#[requires_review]"), ["[MAINT5001] #[requires_review] says why: #[requires_review(reason = \"...\")]"]);
        assert_eq!(
            refusals("#[requires_review(reson = \"x\")]"),
            ["[MAINT0001] the attribute does not read: unknown attribute key `reson`; did you mean `reason`? known keys: reason"]
        );
        assert_eq!(refusals("#[deprecated_after(\"2026-02-30\")]")[0], "[MAINT3001] a date is written YYYY-MM-DD: `2026-02-30` is not a day of the calendar");
        assert_eq!(refusals("#[deprecated_after(\"2026-12-01\")]"), ["[MAINT3003] #[deprecated_after] says why: #[deprecated_after(\"2026-12-01\", reason = \"...\")]"]);
        assert_eq!(refusals("#[owner(\"a\")] #[owner(\"b\")]"), ["[MAINT0003] the attribute is written twice on one item: #[owner]"]);
        let mut errs = SynBuffer::new();
        read_timed_test(&attrs("#[timed_test(max = \"1s\", class = \"unit\")]")[0], &mut errs);
        assert_eq!(errs.finish().unwrap_err().to_string(), "[MAINT0001] the attribute does not read: `max` cannot be used with `class`");
    }

    #[test]
    fn every_key_is_documented() {
        for schema in [vocabulary::sensitivity(), vocabulary::timed_test(), vocabulary::requires_review(), vocabulary::deprecated_after()] {
            let text = schema.describe();
            for key in schema.keys() {
                let line = text.lines().find(|l| l.starts_with(&format!("{key}: "))).unwrap();
                assert!(line.matches(": ").count() >= 2, "{key} has no doc: {line}");
            }
        }
    }
}
