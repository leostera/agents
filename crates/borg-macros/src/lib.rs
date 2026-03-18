mod eval;
mod grade;
mod suite;
mod tool;

use proc_macro::TokenStream;
use syn::{DeriveInput, ItemFn, parse_macro_input};

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

#[proc_macro_attribute]
pub fn grade(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    grade::expand(attr.into(), input)
        .unwrap_or_else(|error| error.into_compile_error())
        .into()
}

#[proc_macro_derive(AgentTool, attributes(agent_tool))]
pub fn agent_tool(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    tool::expand(input)
        .unwrap_or_else(|error| error.into_compile_error())
        .into()
}
