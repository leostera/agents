mod session;
use schemars::JsonSchema;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::agent::error::{AgentError, AgentResult};

pub use session::{
    AgentBuilder, AgentEvent, AgentInput, AgentRunInput, AgentRunOutput, ExecutionProfile,
    PreparedRequest, SessionAgent,
};

/// Typed interface for an agent.
///
/// The trait is centered around a turn-based API:
///
/// - [`send`](Self::send) queues input into the session
/// - [`next`](Self::next) advances the session and yields the next event
/// - [`call`](Self::call), [`cast`](Self::cast), [`steer`](Self::steer), and [`cancel`](Self::cancel)
///   are convenience helpers built on top of `send` + `next`
/// - [`spawn`](Self::spawn) adapts the same agent into a background task with channels
///
/// Most users should implement this trait by delegating to [`SessionAgent`],
/// either manually or through `#[derive(Agent)]`.
///
/// # One-shot call
///
/// ```rust,no_run
/// # use std::sync::Arc;
/// # use agents::{Agent, AgentEvent, AgentInput, AgentResult, LlmRunner, SessionAgent};
/// # struct EchoAgent {
/// #     inner: SessionAgent<String, (), (), String>,
/// # }
/// # impl EchoAgent {
/// #     async fn new(llm: Arc<LlmRunner>) -> anyhow::Result<Self> {
/// #         Ok(Self {
/// #             inner: SessionAgent::builder().with_llm_runner(llm).build()?,
/// #         })
/// #     }
/// # }
/// # impl Agent for EchoAgent {
/// #     type Input = String;
/// #     type ToolCall = ();
/// #     type ToolResult = ();
/// #     type Output = String;
/// #     async fn send(&mut self, input: AgentInput<Self::Input>) -> AgentResult<()> {
/// #         self.inner.send(input).await
/// #     }
/// #     async fn next(
/// #         &mut self,
/// #     ) -> AgentResult<Option<AgentEvent<Self::ToolCall, Self::ToolResult, Self::Output>>> {
/// #         self.inner.next().await
/// #     }
/// #     async fn spawn(
/// #         self,
/// #     ) -> AgentResult<(agents::AgentRunInput<Self::Input>, agents::AgentRunOutput<Self::ToolCall, Self::ToolResult, Self::Output>)> {
/// #         self.inner.spawn().await
/// #     }
/// # }
/// # async fn demo(llm: Arc<LlmRunner>) -> anyhow::Result<()> {
/// let mut agent = EchoAgent::new(llm).await?;
/// let reply = agent.call("hello".to_string()).await?;
/// assert_eq!(reply, "hello");
/// # Ok(()) }
/// ```
///
/// # Spawned session
///
/// ```rust,no_run
/// use std::sync::Arc;
///
/// use agents::{Agent, AgentEvent, AgentInput, AgentResult, AgentRunInput, AgentRunOutput, LlmRunner, SessionAgent};
///
/// struct EchoAgent {
///     inner: SessionAgent<String, (), (), String>,
/// }
///
/// impl EchoAgent {
///     async fn new(llm: Arc<LlmRunner>) -> anyhow::Result<Self> {
///         Ok(Self {
///             inner: SessionAgent::builder().with_llm_runner(llm).build()?,
///         })
///     }
/// }
///
/// impl Agent for EchoAgent {
///     type Input = String;
///     type ToolCall = ();
///     type ToolResult = ();
///     type Output = String;
///
///     async fn send(&mut self, input: AgentInput<Self::Input>) -> AgentResult<()> {
///         self.inner.send(input).await
///     }
///
///     async fn next(
///         &mut self,
///     ) -> AgentResult<Option<AgentEvent<Self::ToolCall, Self::ToolResult, Self::Output>>> {
///         self.inner.next().await
///     }
///
///     async fn spawn(
///         self,
///     ) -> AgentResult<(AgentRunInput<Self::Input>, AgentRunOutput<Self::ToolCall, Self::ToolResult, Self::Output>)> {
///         self.inner.spawn().await
///     }
/// }
///
/// # async fn demo(llm: Arc<LlmRunner>) -> anyhow::Result<()> {
/// let agent = EchoAgent::new(llm).await?;
/// let (input, mut events): (AgentRunInput<String>, AgentRunOutput<(), (), String>) =
///     agent.spawn().await?;
/// input.send(AgentInput::Message("hello".to_string())).await?;
///
/// while let Some(event) = events.recv().await {
///     match event? {
///         AgentEvent::Completed { reply, .. } => {
///             assert_eq!(reply, "hello");
///             break;
///         }
///         _ => {}
///     }
/// }
/// # Ok(()) }
/// ```
#[allow(async_fn_in_trait)]
pub trait Agent: Send + 'static {
    /// Input message type accepted by the agent.
    type Input: Clone + Serialize + DeserializeOwned + Send + Sync + 'static;
    /// Tool call type emitted by the agent.
    type ToolCall: Clone + Serialize + DeserializeOwned + Send + Sync + 'static;
    /// Tool result type returned into the agent after execution.
    type ToolResult: Clone + Serialize + DeserializeOwned + Send + Sync + 'static;
    /// Final structured reply type produced by the agent.
    type Output: Clone + Serialize + DeserializeOwned + JsonSchema + Send + Sync + 'static;

    /// Sends an input into the session.
    async fn send(&mut self, input: AgentInput<Self::Input>) -> AgentResult<()>;

    /// Advances the session and yields the next event, if any.
    async fn next(
        &mut self,
    ) -> AgentResult<Option<AgentEvent<Self::ToolCall, Self::ToolResult, Self::Output>>>;

    /// Sends a normal user message without waiting for completion.
    async fn cast(&mut self, input: Self::Input) -> AgentResult<()> {
        self.send(AgentInput::Message(input)).await
    }

    /// Sends a normal user message and waits for the terminal reply.
    async fn call(&mut self, input: Self::Input) -> AgentResult<Self::Output> {
        self.send(AgentInput::Message(input)).await?;
        loop {
            match self.next().await? {
                Some(AgentEvent::Completed { reply, .. }) => return Ok(reply),
                Some(AgentEvent::Cancelled) => return Err(AgentError::Cancelled),
                Some(_) => {}
                None => {
                    return Err(AgentError::Internal {
                        message: "agent ended turn without a terminal event".to_string(),
                    });
                }
            }
        }
    }

    /// Sends steering input and waits for the resulting terminal reply.
    async fn steer(&mut self, input: Self::Input) -> AgentResult<Self::Output> {
        self.send(AgentInput::Steer(input)).await?;
        loop {
            match self.next().await? {
                Some(AgentEvent::Completed { reply, .. }) => return Ok(reply),
                Some(AgentEvent::Cancelled) => return Err(AgentError::Cancelled),
                Some(_) => {}
                None => {
                    return Err(AgentError::Internal {
                        message: "agent ended steered turn without a terminal event".to_string(),
                    });
                }
            }
        }
    }

    /// Requests cancellation and waits until the session observes it.
    async fn cancel(&mut self) -> AgentResult<()> {
        self.send(AgentInput::Cancel).await?;
        loop {
            match self.next().await? {
                Some(AgentEvent::Cancelled) => return Ok(()),
                Some(AgentEvent::Completed { .. }) => {
                    return Err(AgentError::Internal {
                        message: "cancel completed without observing cancellation".to_string(),
                    });
                }
                Some(_) => {}
                None => {
                    return Err(AgentError::Internal {
                        message: "agent ended without observing cancellation".to_string(),
                    });
                }
            }
        }
    }

    /// Spawns the agent as a background task and exposes channel-based I/O.
    async fn spawn(
        self,
    ) -> AgentResult<(
        AgentRunInput<Self::Input>,
        AgentRunOutput<Self::ToolCall, Self::ToolResult, Self::Output>,
    )>
    where
        Self: Sized,
        Self: Sized;
}
