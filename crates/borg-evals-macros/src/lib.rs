mod eval;
mod suite;

use proc_macro::TokenStream;
use syn::{ItemFn, parse_macro_input};

#[proc_macro_attribute]
pub fn suite(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    suite::expand(attr.into(), input)
        .unwrap_or_else(|error| error.into_compile_error())
        .into()
}

#[proc_macro_attribute]
pub fn eval(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    eval::expand(attr.into(), input)
        .unwrap_or_else(|error| error.into_compile_error())
        .into()
}
