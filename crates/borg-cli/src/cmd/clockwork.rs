use anyhow::Result;
use clap::{Subcommand, ValueEnum};
use serde_json::{Value, json};

use crate::app::BorgCliApp;

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum JobKindArg {
    Once,
    Cron,
}

impl JobKindArg {
    fn as_str(self) -> &'static str {
        match self {
            Self::Once => "once",
            Self::Cron => "cron",
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum ClockworkCommand {
    #[command(about = "List clockwork jobs")]
    List {
        #[arg(long, default_value_t = 200)]
        limit: usize,
        #[arg(long, help = "Optional status filter")]
        status: Option<String>,
    },
    #[command(about = "Get one clockwork job")]
    Get {
        #[arg(help = "Job id")]
        job_id: String,
    },
    #[command(about = "Create a clockwork job")]
    Create {
        #[arg(long)]
        actor_id: String,
        #[arg(long)]
        session_id: String,
        #[arg(long, value_enum)]
        kind: JobKindArg,
        #[arg(long, help = "RFC3339 UTC next run timestamp")]
        next_run_at: Option<String>,
        #[arg(long, help = "Cron expression (for kind=cron)")]
        cron: Option<String>,
        #[arg(long, help = "RFC3339 UTC run timestamp (for kind=once)")]
        run_at: Option<String>,
        #[arg(long, default_value = "{}")]
        payload_json: String,
        #[arg(long, default_value = "{}")]
        headers_json: String,
    },
    #[command(about = "Update a clockwork job")]
    Update {
        #[arg(help = "Job id")]
        job_id: String,
        #[arg(long)]
        actor_id: Option<String>,
        #[arg(long)]
        session_id: Option<String>,
        #[arg(long, value_enum)]
        kind: Option<JobKindArg>,
        #[arg(long)]
        next_run_at: Option<String>,
        #[arg(long)]
        cron: Option<String>,
        #[arg(long)]
        run_at: Option<String>,
        #[arg(long)]
        payload_json: Option<String>,
        #[arg(long)]
        headers_json: Option<String>,
    },
    #[command(about = "Pause a clockwork job")]
    Pause { job_id: String },
    #[command(about = "Resume a clockwork job")]
    Resume { job_id: String },
    #[command(about = "Cancel a clockwork job")]
    Cancel { job_id: String },
}

pub async fn run(app: &BorgCliApp, cmd: ClockworkCommand) -> Result<()> {
    let db = app.open_config_db().await?;
    db.migrate().await?;

    let output = match cmd {
        ClockworkCommand::List { limit, status } => {
            let jobs = db.list_clockwork_jobs(limit, status.as_deref()).await?;
            json!({ "ok": true, "entity": "clockwork_jobs", "items": jobs })
        }
        ClockworkCommand::Get { job_id } => {
            let job = db.get_clockwork_job(&job_id).await?;
            json!({ "ok": true, "entity": "clockwork_jobs", "item": job })
        }
        ClockworkCommand::Create {
            actor_id,
            session_id,
            kind,
            next_run_at,
            cron,
            run_at,
            payload_json,
            headers_json,
        } => {
            if actor_id.trim().is_empty() || session_id.trim().is_empty() {
                anyhow::bail!("actor_id and session_id are required");
            }

            let payload = parse_json("payload_json", &payload_json)?;
            let headers = parse_json("headers_json", &headers_json)?;
            let job_id = format!("borg:clockwork_job:{}", uuid::Uuid::new_v4());
            let schedule_spec = build_schedule_spec(kind, run_at, cron)?;
            let next_run_at = if kind.as_str() == "once" {
                next_run_at.or_else(|| {
                    schedule_spec
                        .get("run_at")
                        .and_then(Value::as_str)
                        .map(ToString::to_string)
                })
            } else {
                next_run_at
            };

            db.create_clockwork_job(&borg_db::CreateClockworkJobInput {
                job_id: job_id.clone(),
                kind: kind.as_str().to_string(),
                target_actor_id: actor_id,
                target_session_id: session_id,
                message_type: "BorgMessage".to_string(),
                payload,
                headers,
                schedule_spec,
                next_run_at,
            })
            .await?;

            let job = db.get_clockwork_job(&job_id).await?;
            json!({ "ok": true, "entity": "clockwork_jobs", "item": job })
        }
        ClockworkCommand::Update {
            job_id,
            actor_id,
            session_id,
            kind,
            next_run_at,
            cron,
            run_at,
            payload_json,
            headers_json,
        } => {
            let schedule_spec = if kind.is_some() || run_at.is_some() || cron.is_some() {
                Some(build_schedule_spec(
                    kind.unwrap_or(JobKindArg::Once),
                    run_at,
                    cron,
                )?)
            } else {
                None
            };
            let payload = payload_json
                .as_deref()
                .map(|raw| parse_json("payload_json", raw))
                .transpose()?;
            let headers = headers_json
                .as_deref()
                .map(|raw| parse_json("headers_json", raw))
                .transpose()?;

            db.update_clockwork_job(
                &job_id,
                &borg_db::UpdateClockworkJobInput {
                    kind: kind.map(|value| value.as_str().to_string()),
                    target_actor_id: actor_id,
                    target_session_id: session_id,
                    message_type: None,
                    payload,
                    headers,
                    schedule_spec,
                    next_run_at: next_run_at.map(Some),
                },
            )
            .await?;

            let job = db.get_clockwork_job(&job_id).await?;
            json!({ "ok": true, "entity": "clockwork_jobs", "item": job })
        }
        ClockworkCommand::Pause { job_id } => {
            db.set_clockwork_job_status(&job_id, "paused").await?;
            let job = db.get_clockwork_job(&job_id).await?;
            json!({ "ok": true, "entity": "clockwork_jobs", "item": job })
        }
        ClockworkCommand::Resume { job_id } => {
            db.set_clockwork_job_status(&job_id, "active").await?;
            let job = db.get_clockwork_job(&job_id).await?;
            json!({ "ok": true, "entity": "clockwork_jobs", "item": job })
        }
        ClockworkCommand::Cancel { job_id } => {
            db.set_clockwork_job_status(&job_id, "cancelled").await?;
            let job = db.get_clockwork_job(&job_id).await?;
            json!({ "ok": true, "entity": "clockwork_jobs", "item": job })
        }
    };

    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}

fn parse_json(field: &str, raw: &str) -> Result<Value> {
    serde_json::from_str(raw).map_err(|err| anyhow::anyhow!("invalid {field} JSON: {err}"))
}

fn build_schedule_spec(
    kind: JobKindArg,
    run_at: Option<String>,
    cron: Option<String>,
) -> Result<Value> {
    match kind {
        JobKindArg::Once => {
            let run_at =
                run_at.ok_or_else(|| anyhow::anyhow!("--run-at is required for --kind once"))?;
            Ok(json!({ "kind": "once", "run_at": run_at }))
        }
        JobKindArg::Cron => {
            let cron = cron.ok_or_else(|| anyhow::anyhow!("--cron is required for --kind cron"))?;
            Ok(json!({ "kind": "cron", "cron": cron }))
        }
    }
}
