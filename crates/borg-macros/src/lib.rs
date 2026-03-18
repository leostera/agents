mod agent;
mod eval;
mod grade;
mod suite;
mod tool;

use proc_macro::TokenStream;
use syn::{DeriveInput, ItemFn, parse_macro_input};

/// Defines an eval suite factory.
///
/// ```rust,no_run
/// use agents::{SessionAgent, suite};
/// use agents::evals::EvalContext;
/// use anyhow::Result;
///
/// type BasicAgent = SessionAgent<String, (), (), String>;
///
/// #[suite(kind = "regression", agent = new_agent)]
/// async fn new_agent(ctx: EvalContext<()>) -> Result<BasicAgent> {
///     Ok(SessionAgent::builder().with_llm_runner(ctx.llm_runner()).build()?)
/// }
/// ```
#[proc_macro_attribute]
pub fn suite(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    suite::expand(attr.into(), input)
        .unwrap_or_else(|error| error.into_compile_error())
        .into()
}

/// Defines an eval.
///
/// ```rust,no_run
/// use agents::{eval, trajectory, user};
/// use agents::evals::{EvalContext, Trajectory};
/// use anyhow::Result;
///
/// #[eval(agent = BasicAgent, desc = "echoes the input", tags = ["smoke"])]
/// async fn smoke(_ctx: EvalContext<()>) -> Result<Trajectory<BasicAgent, ()>> {
///     Ok(trajectory![user!("hello")])
/// }
/// ```
#[proc_macro_attribute]
pub fn eval(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    eval::expand(attr.into(), input)
        .unwrap_or_else(|error| error.into_compile_error())
        .into()
}

/// Defines a reusable deterministic grader helper.
///
/// ```rust,no_run
/// use agents::grade;
/// use agents::evals::{AgentTrial, EvalContext, GradeResult};
/// use anyhow::Result;
///
/// #[grade(name = "echoes_input")]
/// async fn echoes_input(
///     trial: AgentTrial<String>,
///     _ctx: EvalContext<()>,
/// ) -> Result<GradeResult> {
///     let reply = trial.final_reply.unwrap_or_default();
///     Ok(GradeResult {
///         score: if reply == "hello" { 1.0 } else { 0.0 },
///         summary: "agent should echo the input".to_string(),
///         evidence: serde_json::json!({ "reply": reply }),
///     })
/// }
/// ```
#[proc_macro_attribute]
pub fn grade(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    grade::expand(attr.into(), input)
        .unwrap_or_else(|error| error.into_compile_error())
        .into()
}

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
/// #[derive(agents::Tool)]
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
