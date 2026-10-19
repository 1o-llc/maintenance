//! Sensitivity levels: a big-bad-bool word, one named flag per level, its bit its rank.
//!
//! The levels this crate ships are [`Sensitivity`]'s flags: `public`, `internal`, `confidential`, `secret`,
//! `key_material`, lowest first. A workspace with levels of its own declares its word once with
//! `maintenance_macros::sensitivity_levels!`, which reads the declaration with [`parse_declaration`] (so the macro
//! and the checker read it the same way) and makes the same kind of word; an item names it with
//! `#[sensitivity(level, levels = path::to::Word)]`.

use big_bad_bool::{bitpack, BitFlag};
use liaise_proc_macro::SynBuffer;

use crate::codes::Code;

/// The levels this crate ships, one flag each; a higher bit is a more sensitive level.
#[bitpack(storage = u32, version = 1)]
pub struct Sensitivity {
    #[bits(5)]
    #[describe("one flag per sensitivity level, lowest first: a higher bit is a more sensitive level")]
    #[flags(public = 0, internal = 1, confidential = 2, secret = 3, key_material = 4)]
    level: u8,
}

/// A level word: its flags are its levels.
pub trait Levels {
    /// The word's flags, from its `#[flags(..)]`.
    const FLAGS: &'static [BitFlag];
}

impl Levels for Sensitivity {
    const FLAGS: &'static [BitFlag] = Sensitivity::FLAGS;
}

/// The bit (rank) of the level called `name`, if the word has one. A `const fn`, so a level the macro cannot see
/// (one of a word the user declared) is still checked while the crate compiles.
pub const fn rank_of(flags: &[BitFlag], name: &str) -> Option<u32> {
    let mut i = 0;
    while i < flags.len() {
        if str_eq(flags[i].name, name) {
            return Some(flags[i].bit);
        }
        i += 1;
    }
    None
}

/// [`rank_of`], or 0 when there is none (the compile already failed on it then).
pub const fn rank_or_zero(flags: &[BitFlag], name: &str) -> u32 {
    match rank_of(flags, name) {
        Some(rank) => rank,
        None => 0,
    }
}

const fn str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// A level table as plain data: names, lowest first, each with its bit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelTable {
    /// The word's name: `Sensitivity` for the shipped one.
    pub word: String,
    /// `(name, bit)`, sorted by bit.
    pub levels: Vec<(String, u32)>,
}

impl LevelTable {
    /// The levels this crate ships.
    pub fn shipped() -> Self {
        Self::from_flags("Sensitivity", Sensitivity::FLAGS)
    }

    /// A table from a word's flags.
    pub fn from_flags(word: &str, flags: &[BitFlag]) -> Self {
        let mut levels: Vec<(String, u32)> = flags.iter().map(|f| (f.name.to_owned(), f.bit)).collect();
        levels.sort_by_key(|(_, bit)| *bit);
        Self { word: word.to_owned(), levels }
    }

    /// The names, lowest first.
    pub fn names(&self) -> Vec<String> {
        self.levels.iter().map(|(n, _)| n.clone()).collect()
    }

    /// The level's place in the table, lowest first (0 for the lowest).
    pub fn place(&self, name: &str) -> Option<usize> {
        self.levels.iter().position(|(n, _)| n == name)
    }

    /// The level's bit.
    pub fn bit(&self, name: &str) -> Option<u32> {
        self.levels.iter().find(|(n, _)| n == name).map(|(_, b)| *b)
    }

    /// `public, internal, ...` for a message.
    pub fn listed(&self) -> String {
        self.names().join(", ")
    }
}

/// `sensitivity_levels! { pub TradeLevels { public = 0, internal = 1, trade_secret = 2 } }`, read.
#[derive(Clone)]
pub struct LevelsDecl {
    pub vis: syn::Visibility,
    pub name: syn::Ident,
    /// As written, in order.
    pub levels: Vec<(syn::Ident, u32)>,
}

impl LevelsDecl {
    /// The table this declaration makes.
    pub fn table(&self) -> LevelTable {
        let mut levels: Vec<(String, u32)> = self.levels.iter().map(|(n, b)| (n.to_string(), *b)).collect();
        levels.sort_by_key(|(_, bit)| *bit);
        LevelTable { word: self.name.to_string(), levels }
    }
}

/// Read a `sensitivity_levels!` body. Every problem is reported, each coded and at the token it is about.
pub fn parse_declaration(tokens: proc_macro2::TokenStream) -> syn::Result<LevelsDecl> {
    use syn::parse::Parser;
    let read = |input: syn::parse::ParseStream<'_>| -> syn::Result<(syn::Visibility, syn::Ident, Vec<(syn::Ident, syn::LitInt)>)> {
        let vis: syn::Visibility = input.parse()?;
        let name: syn::Ident = input.parse()?;
        let body;
        syn::braced!(body in input);
        let mut pairs = Vec::new();
        while !body.is_empty() {
            let level: syn::Ident = body.parse()?;
            body.parse::<syn::Token![=]>()?;
            let bit: syn::LitInt = body.parse()?;
            pairs.push((level, bit));
            if body.is_empty() {
                break;
            }
            body.parse::<syn::Token![,]>()?;
        }
        Ok((vis, name, pairs))
    };
    let (vis, name, pairs) = read
        .parse2(tokens)
        .map_err(|e| liaise_proc_macro::err_at_ctx(e.span(), Code::LevelsUnreadable, e))?;
    let mut errs = SynBuffer::new();
    liaise_proc_macro::ensure!(errs, !pairs.is_empty(), &name => Code::LevelsEmpty);
    let mut levels: Vec<(syn::Ident, u32)> = Vec::new();
    for (level, lit) in pairs {
        let Some(bit) = errs.absorb(lit.base10_parse::<u32>().map_err(|e| liaise_proc_macro::err_spanned_ctx(&lit, Code::LevelBitTooHigh, e))) else {
            continue;
        };
        if !liaise_proc_macro::ensure!(errs, bit < 32, &lit => Code::LevelBitTooHigh, format!("`{level}` is at bit {bit}")) {
            continue;
        }
        if let Some((other, _)) = levels.iter().find(|(_, b)| *b == bit) {
            errs.push_spanned_ctx(&lit, Code::LevelBitTaken, format!("`{level}` and `{other}` are both at bit {bit}"));
            continue;
        }
        if levels.iter().any(|(n, _)| n == &level) {
            errs.push_spanned_ctx(&level, Code::LevelNameTaken, format!("`{level}`"));
            continue;
        }
        levels.push((level, bit));
    }
    errs.finish_with(LevelsDecl { vis, name, levels })
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn the_shipped_levels_are_a_word_lowest_first() {
        let t = LevelTable::shipped();
        assert_eq!(t.names(), ["public", "internal", "confidential", "secret", "key_material"]);
        assert_eq!(rank_of(Sensitivity::FLAGS, "secret"), Some(3));
        assert_eq!(rank_of(Sensitivity::FLAGS, "nope"), None);
        let word = Sensitivity::ZERO.with_level_secret();
        assert!(word.has_level_secret() && !word.has_level_public());
        const SECRET: u32 = rank_or_zero(<Sensitivity as Levels>::FLAGS, "secret");
        assert_eq!(SECRET, 3, "readable in a const");
    }

    #[test]
    fn a_declaration_reads_into_a_table() {
        let d = parse_declaration(quote!(pub TradeLevels { public = 0, trade_secret = 4, internal = 1 })).unwrap();
        assert_eq!(d.name, "TradeLevels");
        assert_eq!(d.table().names(), ["public", "internal", "trade_secret"]);
        assert_eq!(d.table().place("trade_secret"), Some(2));
        assert_eq!(d.table().bit("trade_secret"), Some(4));
    }

    #[test]
    fn every_problem_in_a_declaration_is_reported_coded() {
        let all = |t| parse_declaration(t).err().unwrap().into_iter().map(|e| e.to_string()).collect::<Vec<_>>();
        assert_eq!(
            all(quote!(L { a = 0, b = 0, a = 1, c = 40 })),
            [
                "[MAINT1011] two levels are declared at one bit: `b` and `a` are both at bit 0",
                "[MAINT1012] a level is declared twice: `a`",
                "[MAINT1013] a level's bit is its rank in a u32 word: 0 to 31: `c` is at bit 40",
            ]
        );
        assert_eq!(all(quote!(L {})), ["[MAINT1010] sensitivity_levels! declares at least one level"]);
        assert!(all(quote!(L { a: 0 }))[0].starts_with("[MAINT1014] sensitivity_levels! reads"));
    }
}
