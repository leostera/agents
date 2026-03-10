use anyhow::{Context, Result};
use chrono::Utc;

use crate::utils::parse_ts;
use crate::{BorgDb, MessageRecord};
use borg_core::{
    CorrelationId, EndpointUri, MessageId, MessagePayload, ProcessingState, WorkspaceId,
};

impl BorgDb {
    /// Persist a new message in the `messages` table.
    /// This is the "delivery" point as per RFD0033.
    pub async fn insert_message(
        &self,
        message_id: &MessageId,
        workspace_id: &WorkspaceId,
        sender_id: &EndpointUri,
        receiver_id: &EndpointUri,
        payload: &MessagePayload,
        conversation_id: Option<&str>,
        in_reply_to_message_id: Option<&MessageId>,
        correlation_id: Option<&CorrelationId>,
    ) -> Result<()> {
        let message_id = message_id.to_string();
        let workspace_id = workspace_id.to_string();
        let sender_id = sender_id.to_string();
        let receiver_id = receiver_id.to_string();
        let payload_json = payload.to_json()?;
        let in_reply_to = in_reply_to_message_id.map(|id| id.to_string());
        let correlation = correlation_id.map(|id| id.to_string());
        let now = Utc::now().to_rfc3339();
        let state = ProcessingState::Pending.to_string();

        sqlx::query!(
            r#"
            INSERT INTO messages (
                message_id, workspace_id, sender_id, receiver_id,
                payload_json, conversation_id, in_reply_to_message_id,
                correlation_id, delivered_at, processing_state
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
            message_id,
            workspace_id,
            sender_id,
            receiver_id,
            payload_json,
            conversation_id,
            in_reply_to,
            correlation,
            now,
            state
        )
        .execute(self.pool())
        .await
        .context("failed to insert message")?;

        Ok(())
    }

    /// Transition a message to the `processed` state.
    pub async fn mark_message_processed(&self, message_id: &MessageId) -> Result<()> {
        let id = message_id.to_string();
        let now = Utc::now().to_rfc3339();
        let state = ProcessingState::Processed.to_string();

        sqlx::query!(
            r#"
            UPDATE messages
            SET processing_state = ?1,
                processed_at = ?2
            WHERE message_id = ?3
              AND processing_state = 'pending'
            "#,
            state,
            now,
            id
        )
        .execute(self.pool())
        .await
        .context("failed to mark message as processed")?;

        Ok(())
    }

    /// Transition a message to the `failed` state.
    pub async fn mark_message_failed(
        &self,
        message_id: &MessageId,
        failure_code: &str,
        failure_message: &str,
    ) -> Result<()> {
        let id = message_id.to_string();
        let now = Utc::now().to_rfc3339();
        let state = ProcessingState::Failed.to_string();

        sqlx::query!(
            r#"
            UPDATE messages
            SET processing_state = ?1,
                failed_at = ?2,
                failure_code = ?3,
                failure_message = ?4
            WHERE message_id = ?5
              AND processing_state = 'pending'
            "#,
            state,
            now,
            failure_code,
            failure_message,
            id
        )
        .execute(self.pool())
        .await
        .context("failed to mark message as failed")?;

        Ok(())
    }

    pub async fn get_message(&self, message_id: &MessageId) -> Result<Option<MessageRecord>> {
        let id = message_id.to_string();
        let row = sqlx::query!(
            r#"
            SELECT
                message_id as "message_id!: String",
                workspace_id as "workspace_id!: String",
                sender_id as "sender_id!: String",
                receiver_id as "receiver_id!: String",
                payload_json as "payload_json!: String",
                conversation_id,
                in_reply_to_message_id,
                correlation_id,
                delivered_at as "delivered_at!: String",
                processing_state as "processing_state!: String",
                processed_at,
                failed_at,
                failure_code,
                failure_message
            FROM messages
            WHERE message_id = ?1
            LIMIT 1
            "#,
            id,
        )
        .fetch_optional(self.pool())
        .await
        .context("failed to get message")?;

        let Some(row) = row else {
            return Ok(None);
        };

        Ok(Some(MessageRecord {
            message_id: MessageId::parse(&row.message_id)?,
            workspace_id: WorkspaceId::parse(&row.workspace_id)?,
            sender_id: EndpointUri::parse(&row.sender_id)?,
            receiver_id: EndpointUri::parse(&row.receiver_id)?,
            payload: MessagePayload::from_json(&row.payload_json)?,
            conversation_id: row.conversation_id,
            in_reply_to_message_id: row
                .in_reply_to_message_id
                .map(|id| MessageId::parse(&id))
                .transpose()?,
            correlation_id: row
                .correlation_id
                .map(|id| CorrelationId::parse(&id))
                .transpose()?,
            delivered_at: parse_ts(&row.delivered_at)?,
            processing_state: ProcessingState::parse(&row.processing_state)?,
            processed_at: row.processed_at.map(|s| parse_ts(&s)).transpose()?,
            failed_at: row.failed_at.map(|s| parse_ts(&s)).transpose()?,
            failure_code: row.failure_code,
            failure_message: row.failure_message,
        }))
    }

    /// List all pending messages for a specific receiver (e.g. an actor's mailbox).
    pub async fn list_pending_messages(
        &self,
        receiver_id: &EndpointUri,
        limit: usize,
    ) -> Result<Vec<MessageRecord>> {
        let receiver = receiver_id.to_string();
        let limit = i64::try_from(limit).unwrap_or(100);
        let rows = sqlx::query!(
            r#"
            SELECT
                message_id as "message_id!: String",
                workspace_id as "workspace_id!: String",
                sender_id as "sender_id!: String",
                receiver_id as "receiver_id!: String",
                payload_json as "payload_json!: String",
                conversation_id,
                in_reply_to_message_id,
                correlation_id,
                delivered_at as "delivered_at!: String",
                processing_state as "processing_state!: String",
                processed_at,
                failed_at,
                failure_code,
                failure_message
            FROM messages
            WHERE receiver_id = ?1
              AND processing_state = 'pending'
            ORDER BY delivered_at ASC
            LIMIT ?2
            "#,
            receiver,
            limit,
        )
        .fetch_all(self.pool())
        .await
        .context("failed to list pending messages")?;

        rows.into_iter()
            .map(|row| {
                Ok(MessageRecord {
                    message_id: MessageId::parse(&row.message_id)?,
                    workspace_id: WorkspaceId::parse(&row.workspace_id)?,
                    sender_id: EndpointUri::parse(&row.sender_id)?,
                    receiver_id: EndpointUri::parse(&row.receiver_id)?,
                    payload: MessagePayload::from_json(&row.payload_json)?,
                    conversation_id: row.conversation_id,
                    in_reply_to_message_id: row
                        .in_reply_to_message_id
                        .map(|id| MessageId::parse(&id))
                        .transpose()?,
                    correlation_id: row
                        .correlation_id
                        .map(|id| CorrelationId::parse(&id))
                        .transpose()?,
                    delivered_at: parse_ts(&row.delivered_at)?,
                    processing_state: ProcessingState::parse(&row.processing_state)?,
                    processed_at: row.processed_at.map(|s| parse_ts(&s)).transpose()?,
                    failed_at: row.failed_at.map(|s| parse_ts(&s)).transpose()?,
                    failure_code: row.failure_code,
                    failure_message: row.failure_message,
                })
            })
            .collect()
    }

    /// List all messages where the endpoint is either sender or receiver.
    pub async fn list_messages(
        &self,
        endpoint_id: &EndpointUri,
        limit: usize,
    ) -> Result<Vec<MessageRecord>> {
        let endpoint = endpoint_id.to_string();
        let limit = i64::try_from(limit).unwrap_or(100);
        let rows = sqlx::query!(
            r#"
            SELECT
                message_id as "message_id!: String",
                workspace_id as "workspace_id!: String",
                sender_id as "sender_id!: String",
                receiver_id as "receiver_id!: String",
                payload_json as "payload_json!: String",
                conversation_id,
                in_reply_to_message_id,
                correlation_id,
                delivered_at as "delivered_at!: String",
                processing_state as "processing_state!: String",
                processed_at,
                failed_at,
                failure_code,
                failure_message
            FROM messages
            WHERE receiver_id = ?1 OR sender_id = ?1
            ORDER BY delivered_at DESC
            LIMIT ?2
            "#,
            endpoint,
            limit,
        )
        .fetch_all(self.pool())
        .await
        .context("failed to list messages")?;

        let mut records: Vec<MessageRecord> = rows
            .into_iter()
            .map(|row| {
                Ok(MessageRecord {
                    message_id: MessageId::parse(&row.message_id)?,
                    workspace_id: WorkspaceId::parse(&row.workspace_id)?,
                    sender_id: EndpointUri::parse(&row.sender_id)?,
                    receiver_id: EndpointUri::parse(&row.receiver_id)?,
                    payload: MessagePayload::from_json(&row.payload_json)?,
                    conversation_id: row.conversation_id,
                    in_reply_to_message_id: row
                        .in_reply_to_message_id
                        .map(|id| MessageId::parse(&id))
                        .transpose()?,
                    correlation_id: row
                        .correlation_id
                        .map(|id| CorrelationId::parse(&id))
                        .transpose()?,
                    delivered_at: parse_ts(&row.delivered_at)?,
                    processing_state: ProcessingState::parse(&row.processing_state)?,
                    processed_at: row.processed_at.map(|s| parse_ts(&s)).transpose()?,
                    failed_at: row.failed_at.map(|s| parse_ts(&s)).transpose()?,
                    failure_code: row.failure_code,
                    failure_message: row.failure_message,
                })
            })
            .collect::<Result<_>>()?;

        // Reverse to return oldest-first for the UI
        records.reverse();
        Ok(records)
    }

    /// Claim the next pending message for an actor.
    /// In v0.1 this just returns the next message from the ordered pending list.
    pub async fn claim_next_pending_message(
        &self,
        receiver_id: &EndpointUri,
    ) -> Result<Option<MessageRecord>> {
        let messages = self.list_pending_messages(receiver_id, 1).await?;
        Ok(messages.into_iter().next())
    }

    /// Delete all messages for a specific endpoint.
    pub async fn delete_messages_for_endpoint(&self, endpoint_id: &EndpointUri) -> Result<u64> {
        let endpoint = endpoint_id.to_string();
        let deleted = sqlx::query!(
            "DELETE FROM messages WHERE receiver_id = ?1 OR sender_id = ?1",
            endpoint
        )
        .execute(self.pool())
        .await
        .context("failed to delete messages for endpoint")?
        .rows_affected();
        Ok(deleted)
    }
}
