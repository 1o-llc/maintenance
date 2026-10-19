//! `#[timed_test]`: the threshold is decided while the crate compiles (from the attribute, else maintenance.toml),
//! and the test runs under `maintenance::timed`'s clock.

use liaise_proc_macro::SynBuffer;
use maintenance_core::attrs::{self, as_attribute};
use maintenance_core::config::{self, Thresholds};
use maintenance_core::Code;
use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;

/// The classes in force: the shipped ones, with `file`'s on top. A file that does not read is refused at `at`.
fn thresholds_from(file: Option<&std::path::Path>, at: Span) -> syn::Result<Thresholds> {
    match file {
        Some(path) => config::load(path).map_err(|e| liaise_proc_macro::err_at_ctx(at, Code::ThresholdFile, e)).and_then(|t| {
            Thresholds::with_file(&t.timed_test).map_err(|r| liaise_proc_macro::err_at_ctx(at, r.code, format!("{}: {}", path.display(), r.detail)))
        }),
        None => Ok(Thresholds::shipped()),
    }
}

pub(crate) fn timed_test(args: TokenStream, item: TokenStream) -> TokenStream {
    let mut errs = SynBuffer::new();
    let attr = as_attribute(attrs::TIMED_TEST, args.clone());
    let read = attrs::read_timed_test(&attr, &mut errs);
    let attr_span = if args.is_empty() { Span::call_site() } else { args.span() };

    let func: Option<syn::ItemFn> = match syn::parse2::<syn::ItemFn>(item.clone()) {
        Ok(f) if f.sig.inputs.is_empty() && f.sig.generics.params.is_empty() && f.sig.generics.where_clause.is_none() => Some(f),
        Ok(f) => {
            errs.push_spanned_ctx(&f.sig, Code::NotATestFn, format!("`{}`", f.sig.ident));
            None
        }
        Err(_) => {
            errs.push_spanned(&item, Code::NotATestFn);
            None
        }
    };
    let runtime_test = func.as_ref().is_some_and(|f| f.attrs.iter().any(|a| a.path().segments.last().is_some_and(|s| s.ident == "test")));
    if let Some(f) = &func {
        for again in f.attrs.iter().filter(|a| attrs::is_named(a, attrs::TIMED_TEST)) {
            errs.push_spanned_ctx(again.path(), Code::Repeated, "#[timed_test]");
        }
        if f.sig.asyncness.is_some() && !runtime_test {
            errs.push_spanned(f.sig.asyncness, Code::AsyncNeedsRuntime);
        }
    }

    // the threshold, from what the test wrote and the nearest maintenance.toml
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").map(std::path::PathBuf::from).unwrap_or_default();
    let file = config::find(&manifest_dir);
    let root = file.as_ref().and_then(|f| f.parent()).map(|d| d.to_owned()).unwrap_or_else(|| manifest_dir.clone());
    let thresholds = errs.absorb(thresholds_from(file.as_deref(), attr_span));
    let resolved = match (&read, thresholds) {
        (Some(read), Some(thresholds)) => {
            let at = read.max.as_ref().or(read.class.as_ref()).map_or(attr_span, |(_, span)| *span);
            errs.absorb(
                thresholds
                    .resolve(read.max.as_ref().map(|(s, _)| s.as_str()), read.class.as_ref().map(|(s, _)| s.as_str()))
                    .map_err(|r| liaise_proc_macro::err_at_ctx(at, r.code, r.detail)),
            )
        }
        _ => None,
    };

    let (Some(f), Some(resolved), true) = (func, resolved, errs.is_empty()) else {
        let errors = errs.into_compile_error();
        // without a test the item is still whatever it was, so nothing else breaks for this one problem
        return quote!(#errors #item);
    };

    let syn::ItemFn { attrs, vis, sig, block } = f;
    let name = sig.ident.to_string();
    let output = &sig.output;
    let asyncness = &sig.asyncness;
    let max_ms = resolved.max_ms;
    let class = match &resolved.class {
        Some(class) => quote!(::core::option::Option::Some(#class)),
        None => quote!(::core::option::Option::None),
    };
    let from = resolved.from.as_str();
    let root = root.display().to_string();
    let line = quote_spanned!(sig.ident.span()=> ::core::line!());
    // the file is read while the crate compiles: naming it here makes cargo build again when it changes
    let track = file.map(|path| {
        let path = path.display().to_string();
        quote!(const _: &[u8] = ::core::include_bytes!(#path);)
    });
    let test_attr = (!runtime_test).then(|| quote!(#[test]));
    let call = if asyncness.is_some() { quote!(__maintenance_timed().await) } else { quote!(__maintenance_timed()) };
    quote! {
        #test_attr
        #(#attrs)*
        #vis #sig {
            #track
            static __MAINTENANCE_TEST: ::maintenance::timed::TestMeta = ::maintenance::timed::TestMeta {
                module: ::core::module_path!(),
                name: #name,
                file: ::core::file!(),
                line: #line,
                max_ms: #max_ms,
                class: #class,
                threshold_from: #from,
                root: #root,
            };
            #asyncness fn __maintenance_timed() #output #block
            let __maintenance_clock = ::maintenance::timed::start(&__MAINTENANCE_TEST);
            let __maintenance_out = #call;
            __maintenance_clock.finish();
            __maintenance_out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, text: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("maintenance_macros_tests").join(format!("{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("maintenance.toml");
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn a_threshold_file_that_does_not_read_is_refused_coded() {
        let at = Span::call_site();
        let bad = file("bad", "[timed_test]\ndefualt = \"unit\"\n");
        let e = thresholds_from(Some(&bad), at).unwrap_err().to_string();
        assert!(e.starts_with("[MAINT2005] maintenance.toml does not read: "), "{e}");
        assert!(e.ends_with("unknown field `defualt`, expected `default` or `classes`"), "{e}");
        let class = file("class", "[timed_test.classes]\nunit = \"soon\"\n");
        assert!(thresholds_from(Some(&class), at).unwrap_err().to_string().starts_with("[MAINT2002] "));
        let good = file("good", "[timed_test]\ndefault = \"quick\"\n[timed_test.classes]\nquick = \"50ms\"\n");
        assert_eq!(thresholds_from(Some(&good), at).unwrap().resolve(None, None).unwrap().max_ms, 50);
        assert_eq!(thresholds_from(None, at).unwrap(), Thresholds::shipped());
    }
}
