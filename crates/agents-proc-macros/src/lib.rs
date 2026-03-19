mod agent;
mod tool;

use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

/// Derives `agents::agent::Agent` by delegating to an inner field marked with `#[agent]`.
///
/// ```rust
/// use agents::{Agent, SessionAgent};
///
/// #[derive(Agent)]
/// struct EchoAgent {
///     #[agent]
///     inner: SessionAgent<String, (), (), String>,
/// }
/// ```
#[proc_macro_derive(Agent, attributes(agent))]
pub fn agent(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    agent::expand(input)
        .unwrap_or_else(|error| error.into_compile_error())
        .into()
}

/// Derives the typed tool metadata needed by `SessionAgent`.
///
/// ```rust
/// use schemars::JsonSchema;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(agents::Tool)]
/// #[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
/// enum EchoTool {
///     #[agent_tool(name = "echo_text", description = "Echo the provided text.")]
///     Echo { text: String },
/// }
/// ```
#[proc_macro_derive(Tool, attributes(agent_tool))]
pub fn agent_tool(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    tool::expand(input)
        .unwrap_or_else(|error| error.into_compile_error())
        .into()
}
