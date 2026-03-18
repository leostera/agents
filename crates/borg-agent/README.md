# borg-agent

`borg-agent` provides the typed runtime behind the workspace.

Most downstream users should start with the `agents` facade crate instead of depending on this crate directly.

It exposes:

- the `Agent` trait
- `SessionAgent` as the default model-backed implementation
- typed tool execution
- turn-based control with `send`, `next`, `call`, `cancel`, and `spawn`

## Example

```rust
use std::sync::Arc;

use borg_agent::SessionAgent;
use borg_llm::LlmRunner;

fn build_agent(llm: Arc<LlmRunner>) -> anyhow::Result<SessionAgent<String, (), (), String>> {
    Ok(SessionAgent::builder()
        .with_llm_runner(llm)
        .build()?)
}
```
