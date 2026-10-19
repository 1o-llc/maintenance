//! `sensitivity_levels!`: a workspace's own level word, read by the same reader the checker uses.

use proc_macro2::TokenStream;
use quote::quote;

pub(crate) fn sensitivity_levels(input: TokenStream) -> TokenStream {
    let decl = match maintenance_core::levels::parse_declaration(input) {
        Ok(decl) => decl,
        Err(e) => return e.to_compile_error(),
    };
    let (vis, name) = (&decl.vis, &decl.name);
    let width = decl.levels.iter().map(|(_, bit)| bit + 1).max().unwrap_or(1);
    let flags = decl.levels.iter().map(|(level, bit)| {
        let bit = proc_macro2::Literal::u32_unsuffixed(*bit);
        quote!(#level = #bit)
    });
    let width = proc_macro2::Literal::u32_unsuffixed(width);
    quote! {
        #[::big_bad_bool::bitpack(storage = u32, version = 1)]
        #vis struct #name {
            #[bits(#width)]
            #[describe("one flag per sensitivity level, lowest first: a higher bit is a more sensitive level")]
            #[flags(#(#flags),*)]
            level: u32,
        }

        impl ::maintenance_core::levels::Levels for #name {
            const FLAGS: &'static [::big_bad_bool::BitFlag] = #name::FLAGS;
        }
    }
}
