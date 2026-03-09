use super::super::*;

#[Subscription]
impl SubscriptionRoot {
    async fn version(&self) -> impl Stream<Item = &str> {
        stream::once(async move { "0.1.0" })
    }

    /// Streams new messages from an actor timeline as they are appended.
    async fn actor_chat(
        &self,
        ctx: &Context<'_>,
        actor_id: UriScalar,
        after_message_id: Option<UriScalar>,
        poll_interval_ms: Option<i32>,
    ) -> BoxStream<'static, GqlResult<ActorMessageObject>> {
        let setup = async {
            let data = ctx_data(ctx)?.clone();
            let start = resolve_actor_stream_start_offset(
                &data,
                &actor_id.0,
                after_message_id.as_ref().map(|value| &value.0),
            )
            .await?;
            let poll_ms = normalize_poll_interval_ms(poll_interval_ms)?;

            Ok::<_, Error>(actor_message_subscription_stream(
                data, actor_id.0, start, poll_ms,
            ))
        }
        .await;

        match setup {
            Ok(stream) => stream.boxed(),
            Err(err) => stream::once(async move { Err(err) }).boxed(),
        }
    }

    /// Streams actor notifications derived from new timeline messages.
    async fn actor_notifications(
        &self,
        ctx: &Context<'_>,
        actor_id: UriScalar,
        after_message_id: Option<UriScalar>,
        poll_interval_ms: Option<i32>,
        include_user_messages: Option<bool>,
    ) -> BoxStream<'static, GqlResult<ActorNotificationObject>> {
        let setup = async {
            let data = ctx_data(ctx)?.clone();
            let start = resolve_actor_stream_start_offset(
                &data,
                &actor_id.0,
                after_message_id.as_ref().map(|value| &value.0),
            )
            .await?;
            let poll_ms = normalize_poll_interval_ms(poll_interval_ms)?;
            let include_users = include_user_messages.unwrap_or(false);

            let stream = actor_message_subscription_stream(data, actor_id.0, start, poll_ms)
                .filter_map(move |item| async move {
                    match item {
                        Ok(message) => {
                            let is_user = message.record.payload.kind() == "user_text";
                            if is_user && !include_users {
                                return None;
                            }
                            Some(Ok(ActorNotificationObject::from_message(message)))
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
