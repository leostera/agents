use super::super::*;

#[Subscription(use_type_description)]
impl SubscriptionRoot {
    /// Streams new messages from a session timeline as they are appended.
    ///
    /// Usage notes:
    /// - When `afterMessageIndex` is omitted, the stream starts from "now" (tail-follow mode).
    /// - Provide `afterMessageIndex` to replay from a known point.
    /// - `pollIntervalMs` is clamped to safe server bounds.
    ///
    /// Example:
    /// ```graphql
    /// subscription($session: Uri!, $after: Int) {
    ///   sessionChat(sessionId: $session, afterMessageIndex: $after, pollIntervalMs: 500) {
    ///     id
    ///     messageIndex
    ///     messageType
    ///     role
    ///     text
    ///   }
    /// }
    /// ```
    async fn session_chat(
        &self,
        ctx: &Context<'_>,
        session_id: UriScalar,
        after_message_index: Option<i64>,
        poll_interval_ms: Option<i32>,
    ) -> BoxStream<'static, GqlResult<SessionMessageObject>> {
        let setup = async {
            let data = ctx_data(ctx)?.clone();
            let start =
                resolve_session_stream_start_index(&data, &session_id.0, after_message_index)
                    .await?;
            let poll_ms = normalize_poll_interval_ms(poll_interval_ms)?;

            Ok::<_, Error>(session_message_subscription_stream(
                data,
                session_id.0,
                start,
                poll_ms,
            ))
        }
        .await;

        match setup {
            Ok(stream) => stream.boxed(),
            Err(err) => stream::once(async move { Err(err) }).boxed(),
        }
    }

    /// Streams session notifications derived from new timeline messages.
    ///
    /// Usage notes:
    /// - By default, user-authored messages are filtered out.
    /// - Set `includeUserMessages: true` to receive all roles.
    ///
    /// Example:
    /// ```graphql
    /// subscription($session: Uri!) {
    ///   sessionNotifications(sessionId: $session) {
    ///     id
    ///     kind
    ///     title
    ///     text
    ///     sessionMessage { messageIndex messageType role }
    ///   }
    /// }
    /// ```
    async fn session_notifications(
        &self,
        ctx: &Context<'_>,
        session_id: UriScalar,
        after_message_index: Option<i64>,
        poll_interval_ms: Option<i32>,
        include_user_messages: Option<bool>,
    ) -> BoxStream<'static, GqlResult<SessionNotificationObject>> {
        let setup = async {
            let data = ctx_data(ctx)?.clone();
            let start =
                resolve_session_stream_start_index(&data, &session_id.0, after_message_index)
                    .await?;
            let poll_ms = normalize_poll_interval_ms(poll_interval_ms)?;
            let include_users = include_user_messages.unwrap_or(false);

            let stream = session_message_subscription_stream(data, session_id.0, start, poll_ms)
                .filter_map(move |item| async move {
                    match item {
                        Ok(message) => {
                            let parsed = message.parsed();
                            let is_user = parsed.role.as_deref() == Some("user");
                            if is_user && !include_users {
                                return None;
                            }
                            Some(Ok(SessionNotificationObject::from_message(message)))
                        }
                        Err(err) => Some(Err(err)),
                    }
                });

            Ok::<_, Error>(stream)
        }
        .await;

        match setup {
            Ok(stream) => stream.boxed(),
            Err(err) => stream::once(async move { Err(err) }).boxed(),
        }
    }
}
