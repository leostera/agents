use super::super::*;
use borg_core::{ActorId, PortId, WorkspaceId};

#[Object]
impl MutationRoot {
    async fn version(&self) -> &str {
        "0.1.0"
    }

    async fn upsert_actor(
        &self,
        ctx: &Context<'_>,
        input: UpsertActorInput,
    ) -> GqlResult<ActorObject> {
        let data = ctx_data(ctx)?;
        let workspace_id = WorkspaceId::from_id("default");

        data.db
            .upsert_actor(
                &ActorId(input.id.0.clone()),
                &workspace_id,
                &input.name,
                &input.system_prompt,
                "", // TODO: actor_prompt
                input.status.as_db_str(),
            )
            .await
            .map_err(map_anyhow)?;

        if let Some(model) = input
            .model
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            data.db
                .set_actor_model(&ActorId(input.id.0.clone()), model)
                .await
                .map_err(map_anyhow)?;
        }

        let actor = data
            .db
            .get_actor(&ActorId(input.id.0))
            .await
            .map_err(map_anyhow)?
            .ok_or_else(|| gql_error_with_code("actor not found after upsert", "INTERNAL"))?;

        Ok(ActorObject::new(actor))
    }

    async fn delete_actor(&self, ctx: &Context<'_>, id: UriScalar) -> GqlResult<bool> {
        let data = ctx_data(ctx)?;
        let deleted = data
            .db
            .delete_actor(&ActorId(id.0))
            .await
            .map_err(map_anyhow)?;
        Ok(deleted > 0)
    }

    async fn delete_actor_messages(&self, ctx: &Context<'_>, id: UriScalar) -> GqlResult<u64> {
        let data = ctx_data(ctx)?;
        let deleted = data
            .db
            .delete_messages_for_endpoint(&id.0.into())
            .await
            .map_err(map_anyhow)?;
        Ok(deleted)
    }

    async fn upsert_port(
        &self,
        ctx: &Context<'_>,
        input: UpsertPortInput,
    ) -> GqlResult<PortObject> {
        let data = ctx_data(ctx)?;
        let workspace_id = WorkspaceId::from_id("default");
        let settings = input
            .settings
            .map(|value| value.0)
            .unwrap_or_else(|| json!({}));
        if !settings.is_object() {
            return Err(gql_error_with_code(
                "settings must be a JSON object",
                "BAD_REQUEST",
            ));
        }

        let port_id =
            PortId(Uri::from_parts("borg", "port", Some(&input.name)).map_err(map_anyhow)?);

        data.db
            .upsert_port(
                &port_id,
                &workspace_id,
                &input.name,
                &input.provider,
                input.enabled,
                input.allows_guests,
                input
                    .assigned_actor_id
                    .as_ref()
                    .map(|uri| ActorId(uri.0.clone()))
                    .as_ref(),
                &settings,
            )
            .await
            .map_err(map_anyhow)?;

        let port = data
            .db
            .get_port(&input.name)
            .await
            .map_err(map_anyhow)?
            .ok_or_else(|| gql_error_with_code("port not found after upsert", "INTERNAL"))?;
        Ok(PortObject::new(port))
    }

    async fn upsert_port_binding(
        &self,
        ctx: &Context<'_>,
        input: UpsertPortBindingInput,
    ) -> GqlResult<PortBindingObject> {
        let data = ctx_data(ctx)?;
        let workspace_id = WorkspaceId::from_id("default");

        let port_id =
            PortId(Uri::from_parts("borg", "port", Some(&input.port_name)).map_err(map_anyhow)?);

        data.db
            .upsert_port_binding(
                &workspace_id,
                &port_id,
                input.conversation_key.0.as_str(),
                &ActorId(input.actor_id.0.clone()),
            )
            .await
            .map_err(map_anyhow)?;

        Ok(PortBindingObject {
            port_name: input.port_name,
            conversation_key: input.conversation_key.0.to_string(),
            actor_id: input.actor_id.0,
        })
    }

    async fn upsert_port_actor_binding(
        &self,
        ctx: &Context<'_>,
        input: UpsertPortActorBindingInput,
    ) -> GqlResult<PortActorBindingObject> {
        let data = ctx_data(ctx)?;
        let workspace_id = WorkspaceId::from_id("default");
        let port_id =
            PortId(Uri::from_parts("borg", "port", Some(&input.port_name)).map_err(map_anyhow)?);

        if let Some(actor_id) = input.actor_id.as_ref() {
            data.db
                .upsert_port_binding(
                    &workspace_id,
                    &port_id,
                    input.conversation_key.0.as_str(),
                    &ActorId(actor_id.0.clone()),
                )
                .await
                .map_err(map_anyhow)?;
        } else {
            data.db
                .delete_port_binding(&port_id, input.conversation_key.0.as_str())
                .await
                .map_err(map_anyhow)?;
        }

        let binding = data
            .db
            .get_port_binding(&port_id, input.conversation_key.0.as_str())
            .await
            .map_err(map_anyhow)?;

        Ok(PortActorBindingObject {
            port_name: input.port_name,
            conversation_key: input.conversation_key.0.to_string(),
            actor_id: binding.map(|b| b.actor_id.into_uri()),
        })
    }

    async fn upsert_provider(
        &self,
        ctx: &Context<'_>,
        input: UpsertProviderInput,
    ) -> GqlResult<ProviderObject> {
        let data = ctx_data(ctx)?;
        let provider_kind = input
            .provider_kind
            .as_deref()
            .unwrap_or(input.provider.as_str());

        data.db
            .upsert_provider_with_kind(
                &input.provider,
                provider_kind,
                input.api_key.as_deref(),
                input.base_url.as_deref(),
                input.enabled,
                input.default_text_model.as_deref(),
                input.default_audio_model.as_deref(),
            )
            .await
            .map_err(map_anyhow)?;

        let provider = data
            .db
            .get_provider(&input.provider)
            .await
            .map_err(map_anyhow)?
            .ok_or_else(|| gql_error_with_code("provider not found after upsert", "INTERNAL"))?;
        ProviderObject::try_new(provider)
    }

    async fn delete_provider(&self, ctx: &Context<'_>, provider: String) -> GqlResult<bool> {
        let data = ctx_data(ctx)?;
        let deleted = data
            .db
            .delete_provider(&provider)
            .await
            .map_err(map_anyhow)?;
        Ok(deleted > 0)
    }

    async fn upsert_app(&self, ctx: &Context<'_>, input: UpsertAppInput) -> GqlResult<AppObject> {
        let data = ctx_data(ctx)?;
        let auth_config = input
            .auth_config
            .map(|value| value.0)
            .unwrap_or_else(|| json!({}));

        data.db
            .upsert_app_with_metadata(
                &input.id.0,
                &input.name,
                &input.slug,
                &input.description,
                input.status.as_db_str(),
                input.built_in,
                &input.source,
                &input.auth_strategy,
                &auth_config,
                &input.available_secrets,
            )
            .await
            .map_err(map_anyhow)?;

        let app = data
            .db
            .get_app(&input.id.0)
            .await
            .map_err(map_anyhow)?
            .ok_or_else(|| gql_error_with_code("app not found after upsert", "INTERNAL"))?;
        Ok(AppObject::new(app))
    }

    async fn upsert_app_capability(
        &self,
        ctx: &Context<'_>,
        input: UpsertAppCapabilityInput,
    ) -> GqlResult<AppCapabilityObject> {
        let data = ctx_data(ctx)?;
        data.db
            .upsert_app_capability(
                &input.app_id.0,
                &input.capability_id.0,
                &input.name,
                &input.hint,
                &input.mode,
                &input.instructions,
                input.status.as_db_str(),
            )
            .await
            .map_err(map_anyhow)?;

        let capability = data
            .db
            .get_app_capability(&input.app_id.0, &input.capability_id.0)
            .await
            .map_err(map_anyhow)?
            .ok_or_else(|| gql_error_with_code("capability not found after upsert", "INTERNAL"))?;

        Ok(AppCapabilityObject::new(capability))
    }

    async fn upsert_app_connection(
        &self,
        ctx: &Context<'_>,
        input: UpsertAppConnectionInput,
    ) -> GqlResult<AppExternalConnectionObject> {
        let data = ctx_data(ctx)?;
        let connection_json = input
            .connection
            .map(|value| value.0)
            .unwrap_or_else(|| json!({}));

        data.db
            .upsert_app_connection(
                &input.app_id.0,
                &input.connection_id.0,
                input.owner_user_id.as_ref().map(|uri| &uri.0),
                input.provider_account_id.as_deref(),
                input.external_user_id.as_deref(),
                input.status.as_db_str(),
                &connection_json,
            )
            .await
            .map_err(map_anyhow)?;

        let connection = data
            .db
            .get_app_connection(&input.app_id.0, &input.connection_id.0)
            .await
            .map_err(map_anyhow)?
            .ok_or_else(|| gql_error_with_code("connection not found after upsert", "INTERNAL"))?;

        Ok(AppExternalConnectionObject::new(connection))
    }

    async fn upsert_app_secret(
        &self,
        ctx: &Context<'_>,
        input: UpsertAppSecretInput,
    ) -> GqlResult<AppSecretObject> {
        let data = ctx_data(ctx)?;
        data.db
            .upsert_app_secret(
                &input.app_id.0,
                &input.secret_id.0,
                input.connection_id.as_ref().map(|uri| &uri.0),
                &input.key,
                &input.value,
                &input.kind,
            )
            .await
            .map_err(map_anyhow)?;

        let secret = data
            .db
            .get_app_secret(&input.app_id.0, &input.secret_id.0)
            .await
            .map_err(map_anyhow)?
            .ok_or_else(|| gql_error_with_code("secret not found after upsert", "INTERNAL"))?;

        Ok(AppSecretObject::new(secret))
    }

    async fn create_schedule_job(
        &self,
        ctx: &Context<'_>,
        input: CreateScheduleJobInputGql,
    ) -> GqlResult<ScheduleJobObject> {
        let data = ctx_data(ctx)?;
        let create = CreateScheduleJobInput {
            job_id: input.job_id.clone(),
            kind: input.kind,
            target_actor_id: input.actor_id.0.to_string(),
            message_type: input.message_type,
            payload: input.payload.0,
            headers: input
                .headers
                .map(|value| value.0)
                .unwrap_or_else(|| json!({})),
            schedule_spec: input.schedule_spec.0,
            next_run_at: input.next_run_at,
        };

        data.db
            .create_schedule_job(&create)
            .await
            .map_err(map_anyhow)?;

        let job = data
            .db
            .get_schedule_job(&input.job_id)
            .await
            .map_err(map_anyhow)?
            .ok_or_else(|| gql_error_with_code("schedule job not found", "INTERNAL"))?;

        Ok(ScheduleJobObject::new(job))
    }

    async fn update_schedule_job(
        &self,
        ctx: &Context<'_>,
        job_id: String,
        patch: UpdateScheduleJobInputGql,
    ) -> GqlResult<ScheduleJobObject> {
        let data = ctx_data(ctx)?;
        let update = UpdateScheduleJobInput {
            kind: patch.kind,
            target_actor_id: patch.actor_id.map(|uri| uri.0.to_string()),
            message_type: patch.message_type,
            payload: patch.payload.map(|value| value.0),
            headers: patch.headers.map(|value| value.0),
            schedule_spec: patch.schedule_spec.map(|value| value.0),
            next_run_at: patch.next_run_at.map(Some),
        };

        data.db
            .update_schedule_job(&job_id, &update)
            .await
            .map_err(map_anyhow)?;

        let job = data
            .db
            .get_schedule_job(&job_id)
            .await
            .map_err(map_anyhow)?
            .ok_or_else(|| gql_error_with_code("schedule job not found", "NOT_FOUND"))?;

        Ok(ScheduleJobObject::new(job))
    }

    async fn pause_schedule_job(&self, ctx: &Context<'_>, job_id: String) -> GqlResult<bool> {
        let data = ctx_data(ctx)?;
        let updated = data
            .db
            .set_schedule_job_status(&job_id, "paused")
            .await
            .map_err(map_anyhow)?;
        Ok(updated > 0)
    }

    async fn resume_schedule_job(&self, ctx: &Context<'_>, job_id: String) -> GqlResult<bool> {
        let data = ctx_data(ctx)?;
        let updated = data
            .db
            .set_schedule_job_status(&job_id, "active")
            .await
            .map_err(map_anyhow)?;
        Ok(updated > 0)
    }

    async fn cancel_schedule_job(&self, ctx: &Context<'_>, job_id: String) -> GqlResult<bool> {
        let data = ctx_data(ctx)?;
        let updated = data
            .db
            .set_schedule_job_status(&job_id, "cancelled")
            .await
            .map_err(map_anyhow)?;
        Ok(updated > 0)
    }

    async fn create_task(
        &self,
        ctx: &Context<'_>,
        input: CreateTaskInputGql,
    ) -> GqlResult<TaskObject> {
        let data = ctx_data(ctx)?;
        let store = TaskGraphStore::new(data.db.clone());

        let created = store
            .create_task(
                input.actor_id.0.as_str(),
                input.creator_actor_id.0.as_str(),
                CreateTaskInput {
                    title: input.title,
                    description: input.description,
                    definition_of_done: input.definition_of_done,
                    assignee_actor_id: input.assignee_actor_id.0.to_string(),
                    parent_uri: input.parent_uri.map(|uri| uri.0.to_string()),
                    blocked_by: input
                        .blocked_by
                        .into_iter()
                        .map(|uri| uri.0.to_string())
                        .collect(),
                    references: input
                        .references
                        .into_iter()
                        .map(|uri| uri.0.to_string())
                        .collect(),
                    labels: input.labels,
                },
            )
            .await
            .map_err(map_anyhow)?;

        TaskObject::try_new(created)
    }

    async fn update_task(
        &self,
        ctx: &Context<'_>,
        input: UpdateTaskInputGql,
    ) -> GqlResult<TaskObject> {
        let data = ctx_data(ctx)?;
        let store = TaskGraphStore::new(data.db.clone());
        let task = store
            .update_task_fields(
                input.actor_id.0.as_str(),
                input.task_id.0.as_str(),
                TaskPatch {
                    title: input.title,
                    description: input.description,
                    definition_of_done: input.definition_of_done,
                },
            )
            .await
            .map_err(map_anyhow)?;

        TaskObject::try_new(task)
    }

    async fn set_task_status(
        &self,
        ctx: &Context<'_>,
        input: SetTaskStatusInput,
    ) -> GqlResult<TaskObject> {
        let data = ctx_data(ctx)?;
        let store = TaskGraphStore::new(data.db.clone());
        let task = store
            .set_task_status(
                input.actor_id.0.as_str(),
                input.task_id.0.as_str(),
                input.status.into(),
            )
            .await
            .map_err(map_anyhow)?;

        TaskObject::try_new(task)
    }

    async fn run_actor_chat(&self, _input: RunActorChatInput) -> GqlResult<RunActorChatResult> {
        Err(gql_error_with_code(
            "runActorChat is not available in standalone borg-gql",
            "BAD_REQUEST",
        ))
    }

    async fn run_port_http(&self, _input: RunPortHttpInput) -> GqlResult<RunPortHttpResult> {
        Err(gql_error_with_code(
            "runPortHttp is not available in standalone borg-gql",
            "BAD_REQUEST",
        ))
    }
}
