# codemode

`codemode` is an embeddable JavaScript execution and code search engine for agent tool runners.

It provides:
- `CodeMode` as the engine
- `Request` / `Response` as the typed boundary
- package, environment, and native function providers for customization

## Example

```rust
use std::sync::Arc;

use codemode::{CodeMode, CodeModeConfig, SearchCode};

# let runtime = tokio::runtime::Builder::new_current_thread()
#     .enable_all()
#     .build()
#     .expect("tokio runtime");
# runtime.block_on(async {
let codemode = Arc::new(
    CodeMode::builder()
        .with_config(CodeModeConfig::default().multithreaded(true))
        .build()?,
);

let response = codemode
    .search_code(SearchCode {
        query: "fetch".to_string(),
        limit: Some(5),
    })
    .await?;

println!("{} matches", response.matches.len());
# Ok::<(), codemode::CodeModeError>(())
# })?;
# Ok::<(), codemode::CodeModeError>(())
```
