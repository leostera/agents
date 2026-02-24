use anyhow::{Context, Result};
use borg_core::{Capability, Task, TaskKind};
use borg_db::{BorgDb, NewTask};
use borg_ltm::MemoryStore;
use borg_rt::RuntimeEngine;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::{error, info, warn};
use uuid::Uuid;

#[derive(Clone)]
pub struct ExecEngine {
    db: BorgDb,
    memory: MemoryStore,
    runtime: RuntimeEngine,
    worker_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InboxMessage {
    pub user_key: String,
    pub text: String,
    #[serde(default)]
    pub metadata: Value,
}

impl ExecEngine {
    pub fn new(db: BorgDb, memory: MemoryStore, runtime: RuntimeEngine, worker_id: String) -> Self {
        Self {
            db,
            memory,
            runtime,
            worker_id,
        }
    }

    pub async fn enqueue_user_message(&self, msg: InboxMessage) -> Result<String> {
        let payload = serde_json::to_value(msg).context("failed to serialize inbox message")?;
        self.db
            .enqueue_task(NewTask {
                kind: TaskKind::UserMessage,
                payload,
                parent_task_id: None,
            })
            .await
    }

    pub async fn run_once(&self) -> Result<bool> {
        let Some(task) = self.db.claim_next_runnable_task(&self.worker_id).await? else {
            return Ok(false);
        };

        info!(target: "borg_exec", task_id = task.task_id, kind = task.kind.as_str(), "claimed task for execution");
        self.db
            .log_event(
                &task.task_id,
                "task_claimed",
                json!({ "worker_id": self.worker_id }),
            )
            .await?;

        match self.process_task(task.clone()).await {
            Ok(()) => {
                info!(target: "borg_exec", task_id = task.task_id, "task execution completed successfully");
            }
            Err(err) => {
                error!(target: "borg_exec", task_id = task.task_id, error = %err, "task execution failed");
                self.db.fail_task(&task.task_id, err.to_string()).await?;
            }
        }

        Ok(true)
    }

    fn search_capabilities(&self, query: &str) -> Vec<Capability> {
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

    async fn process_task(&self, task: Task) -> Result<()> {
        match task.kind {
            TaskKind::UserMessage => self.process_user_message(task).await,
            _ => {
                warn!(target: "borg_exec", task_id = task.task_id, "unsupported task kind in MVP, auto-completing");
                self.db
                    .complete_task(&task.task_id, json!({"status": "ignored"}))
                    .await
            }
        }
    }

    async fn process_user_message(&self, task: Task) -> Result<()> {
        let msg: InboxMessage = serde_json::from_value(task.payload.clone())
            .context("invalid payload for user_message task")?;

        info!(target: "borg_exec", task_id = task.task_id, user_key = msg.user_key, text = msg.text, "processing user message task");

        let lowered = msg.text.to_lowercase();
        if lowered.starts_with("remember i like ") {
            let pref = msg.text[14..].trim().to_string();
            info!(target: "borg_exec", task_id = task.task_id, preference = pref, "detected preference write intent");
            let props = json!({
                "user_key": msg.user_key,
                "movies_path": pref,
                "natural_key": format!("pref:{}:movies_path", msg.user_key)
            });
            self.memory
                .upsert_entity(
                    "Preference",
                    &format!("pref:{}:movies_path", msg.user_key),
                    &props,
                    Some(&format!("pref:{}:movies_path", msg.user_key)),
                )
                .await?;

            let output = format!("Saved preference. I will use {} for movie downloads.", pref);
            self.db
                .log_event(&task.task_id, "output", json!({ "message": output }))
                .await?;
            self.db
                .complete_task(&task.task_id, json!({ "message": output }))
                .await?;
            return Ok(());
        }

        if lowered.starts_with("download ") {
            let title = msg.text[9..].trim().to_string();
            info!(target: "borg_exec", task_id = task.task_id, movie = title, "detected download intent");

            let existing_movies = self.memory.search(&title, Some("Movie"), 10).await?;
            if let Some(found) = existing_movies.iter().find(|e| {
                e.label.eq_ignore_ascii_case(&title)
                    && e.props
                        .get("downloaded")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
            }) {
                let stored_at = found
                    .props
                    .get("stored_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(unknown path)");
                let message = format!("Already downloaded at {}", stored_at);
                info!(target: "borg_exec", task_id = task.task_id, stored_at, "movie already exists in memory");
                self.db
                    .log_event(&task.task_id, "output", json!({ "message": message }))
                    .await?;
                self.db
                    .complete_task(&task.task_id, json!({ "message": message }))
                    .await?;
                return Ok(());
            }

            let capabilities = self.search_capabilities("torrent");
            info!(target: "borg_exec", task_id = task.task_id, count = capabilities.len(), "capability search returned results");
            self.db
                .log_event(
                    &task.task_id,
                    "capabilities",
                    serde_json::to_value(capabilities.clone())?,
                )
                .await?;

            let prefs = self
                .memory
                .search(&msg.user_key, Some("Preference"), 10)
                .await?;
            let dest = prefs
                .iter()
                .find_map(|e| {
                    if e.props.get("user_key").and_then(|v| v.as_str())
                        == Some(msg.user_key.as_str())
                    {
                        e.props
                            .get("movies_path")
                            .and_then(|v| v.as_str())
                            .map(ToOwned::to_owned)
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| "/tmp/movies".to_string());
            info!(target: "borg_exec", task_id = task.task_id, destination = dest, "resolved download destination");

            let simulated_search = self.runtime.execute(&format!(
                "({{ tool: 'torrents.search', query: {:?}, picks: 1 }})",
                title
            ))?;
            self.db
                .log_event(
                    &task.task_id,
                    "execute",
                    json!({"step": "search", "result": simulated_search.result_json, "duration_ms": simulated_search.duration_ms}),
                )
                .await?;

            let magnet = format!("magnet:?xt=urn:btih:{}", Uuid::new_v4().simple());
            let safe_title = slugify(&title);
            let stored_at = format!("{}/{}.mkv", dest.trim_end_matches('/'), safe_title);

            let simulated_download = self.runtime.execute(&format!(
                "({{ tool: 'torrents.download', magnet: {:?}, dest: {:?}, ok: true }})",
                magnet, stored_at
            ))?;
            self.db
                .log_event(
                    &task.task_id,
                    "execute",
                    json!({"step": "download", "result": simulated_download.result_json, "duration_ms": simulated_download.duration_ms}),
                )
                .await?;

            let torrent_id = self
                .memory
                .upsert_entity(
                    "Torrent",
                    &format!("{} 1080p", title),
                    &json!({ "magnet": magnet, "source": "stub" }),
                    None,
                )
                .await?;
            let movie_id = self
                .memory
                .upsert_entity(
                    "Movie",
                    &title,
                    &json!({ "downloaded": true, "stored_at": stored_at, "title": title }),
                    Some(&format!("movie:{}", title.to_lowercase())),
                )
                .await?;
            let _ = self
                .memory
                .link(&movie_id, "downloaded_from", &torrent_id, &json!({}))
                .await?;

            let output = format!("Downloaded {} to {}", title, stored_at);
            info!(target: "borg_exec", task_id = task.task_id, message = output, "download flow completed");
            self.db
                .log_event(&task.task_id, "output", json!({ "message": output }))
                .await?;
            self.db
                .complete_task(&task.task_id, json!({ "message": output }))
                .await?;
            return Ok(());
        }

        let output = format!("I got your message: {}", msg.text);
        info!(target: "borg_exec", task_id = task.task_id, "fallback response generated");
        self.db
            .log_event(&task.task_id, "output", json!({ "message": output }))
            .await?;
        self.db
            .complete_task(&task.task_id, json!({ "message": output }))
            .await?;

        Ok(())
    }
}

fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_underscore = false;

    for c in input.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            prev_underscore = false;
        } else if !prev_underscore {
            out.push('_');
            prev_underscore = true;
        }
    }

    out.trim_matches('_').to_string()
}
