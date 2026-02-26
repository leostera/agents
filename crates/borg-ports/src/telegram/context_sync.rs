use anyhow::Result;
use borg_core::Uri;
use serde_json::{Value, json};
use teloxide::prelude::*;
use teloxide::types::ChatFullInfo;
use tracing::warn;

use super::TelegramPort;

impl TelegramPort {
    pub(super) async fn refresh_session_contexts(&self) -> Result<()> {
        let sessions = self.exec.list_port_session_ids("telegram").await?;
        for session_id in sessions {
            let Some(chat_id) = Self::chat_id_from_session_id(&session_id) else {
                continue;
            };

            let chat = match self.bot.get_chat(ChatId(chat_id)).await {
                Ok(value) => value,
                Err(err) => {
                    warn!(
                        target: "borg_ports",
                        session_id = %session_id,
                        chat_id,
                        error = %err,
                        "failed to fetch telegram chat during startup refresh"
                    );
                    continue;
                }
            };

            let mut snapshot = json!({
                "chat_id": chat.id.0,
                "chat_type": Self::chat_type_label(&chat),
                "participants": {},
                "member_count": Value::Null,
                "last_message_id": Value::Null,
                "last_thread_id": Value::Null,
            });

            if let Ok(member_count) = self.bot.get_chat_member_count(chat.id).await {
                snapshot["member_count"] = json!(member_count);
            }

            if chat.is_private() {
                let id = chat.id.0.to_string();
                snapshot["participants"][&id] = json!({
                    "id": id,
                    "username": Value::Null,
                    "first_name": Value::Null,
                    "last_name": Value::Null
                });
            }

            if let Ok(admins) = self.bot.get_chat_administrators(chat.id).await {
                for admin in admins {
                    let user = admin.user;
                    let id = user.id.0.to_string();
                    snapshot["participants"][&id] = json!({
                        "id": id,
                        "username": user.username,
                        "first_name": user.first_name,
                        "last_name": user.last_name
                    });
                }
            }

            let merged = Self::merge_session_context(
                self.exec
                    .get_port_session_context("telegram", &session_id)
                    .await?,
                snapshot,
            );
            self.exec
                .upsert_port_session_context("telegram", &session_id, &merged)
                .await?;
        }
        Ok(())
    }

    fn merge_session_context(existing: Option<Value>, snapshot: Value) -> Value {
        let mut out = existing.unwrap_or_else(|| json!({}));
        if out.get("participants").and_then(Value::as_object).is_none() {
            out["participants"] = json!({});
        }

        out["chat_id"] = snapshot.get("chat_id").cloned().unwrap_or(Value::Null);
        out["chat_type"] = snapshot
            .get("chat_type")
            .cloned()
            .unwrap_or_else(|| json!("unknown"));

        if let Some(snapshot_participants) = snapshot.get("participants").and_then(Value::as_object)
        {
            for (id, participant) in snapshot_participants {
                out["participants"][id] = participant.clone();
            }
        }

        if snapshot.get("member_count").is_some() {
            out["member_count"] = snapshot["member_count"].clone();
        }

        out
    }

    fn chat_id_from_session_id(session_id: &Uri) -> Option<i64> {
        let raw = session_id.as_str();
        let prefix = "borg:session:telegram_";
        let value = raw.strip_prefix(prefix)?;
        value.parse::<i64>().ok()
    }

    fn chat_type_label(chat: &ChatFullInfo) -> &'static str {
        if chat.is_private() {
            "private"
        } else if chat.is_group() {
            "group"
        } else if chat.is_supergroup() {
            "supergroup"
        } else if chat.is_channel() {
            "channel"
        } else {
            "unknown"
        }
    }
}
