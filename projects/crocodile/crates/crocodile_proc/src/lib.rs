#![feature(proc_macro_quote)]

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse::Parse, parse_macro_input, Ident};

struct Ability {
    name: Ident,
}

impl Parse for Ability {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let name: Ident = input.parse()?;
        Ok(Ability { name })
    }
}

#[proc_macro]
pub fn define_abilities(input: TokenStream) -> TokenStream {
    let Ability { name } = parse_macro_input!(input as Ability);
    let expanded = quote!( enum {
        A,
        #name
    });

    TokenStream::from(expanded)
}
