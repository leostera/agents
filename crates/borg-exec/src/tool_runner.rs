use anyhow::{Result, anyhow};
use async_trait::async_trait;
use borg_agent::{CapabilitySummary, ToolRequest, ToolResponse, ToolResultData, ToolRunner};
use borg_core::Capability;
use borg_rt::RuntimeEngine;
use serde_json::Value;

pub struct ExecToolRunner {
    runtime: RuntimeEngine,
    capabilities: Vec<Capability>,
}

impl ExecToolRunner {
    pub fn new(runtime: RuntimeEngine, capabilities: Vec<Capability>) -> Self {
        Self {
            runtime,
            capabilities,
        }
    }
}

#[async_trait]
impl ToolRunner for ExecToolRunner {
    async fn run(&self, request: ToolRequest) -> Result<ToolResponse> {
        match request.tool_name.as_str() {
            "execute" => {
                let code = request
                    .arguments
                    .get("code")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("execute tool requires code"))?;
                let result = self.runtime.execute(code)?;
                Ok(ToolResponse {
                    content: ToolResultData::Execution {
                        result: result.result_json.to_string(),
                        duration_ms: result.duration_ms,
                    },
                })
            }
            "search" => {
                let query = request
                    .arguments
                    .get("query")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("search tool requires query"))?;
                let q = query.to_lowercase();
                let matches: Vec<Capability> = self
                    .capabilities
                    .iter()
                    .filter(|cap| {
                        cap.name.contains(&q) || cap.description.to_lowercase().contains(&q)
                    })
                    .cloned()
                    .collect();
                let result = if matches.is_empty() {
                    self.capabilities.clone()
                } else {
                    matches
                };
                Ok(ToolResponse {
                    content: ToolResultData::Capabilities(
                        result
                            .into_iter()
                            .map(|cap| CapabilitySummary {
                                name: cap.name,
                                signature: cap.signature,
                                description: cap.description,
                            })
                            .collect(),
                    ),
                })
            }
            _ => Ok(ToolResponse {
                content: ToolResultData::Error {
                    message: format!("unknown tool {}", request.tool_name),
                },
            }),
        }
    }
}

pub fn search_capabilities(query: &str) -> Vec<Capability> {
    let q = query.to_lowercase();
    let catalog = vec![
        Capability {
            name: "torrents.search".to_string(),
            signature: "(query: string) => Promise<TorrentResult[]>".to_string(),
            description: "Searches torrent providers by title keywords".to_string(),
        },
        Capability {
            name: "torrents.download".to_string(),
            signature: "(magnet: string, dest: string) => Promise<DownloadReceipt>".to_string(),
            description: "Downloads a magnet link into a destination path".to_string(),
        },
        Capability {
            name: "memory.upsert".to_string(),
            signature: "(entity: Entity) => Promise<string>".to_string(),
            description: "Upserts an entity into long-term memory".to_string(),
        },
        Capability {
            name: "memory.link".to_string(),
            signature: "(from: string, rel: string, to: string) => Promise<string>".to_string(),
            description: "Creates a relation between entities".to_string(),
        },
    ];

    let filtered: Vec<Capability> = catalog
        .clone()
        .into_iter()
        .filter(|c| c.name.contains(&q) || c.description.to_lowercase().contains(&q))
        .collect();

    if filtered.is_empty() {
        catalog
    } else {
        filtered
    }
}
