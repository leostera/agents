use super::super::*;

#[Object(use_type_description)]
impl MutationRoot {
    /// Creates or updates an actor.
    ///
    /// Example:
    /// ```graphql
    /// mutation($id: Uri!, $behavior: Uri!) {
    ///   upsertActor(input: {
    ///     id: $id
    ///     name: "Planner"
    ///     systemPrompt: "You plan work."
    ///     defaultBehaviorId: $behavior
    ///     status: "RUNNING"
    ///   }) { id name status }
    /// }
    /// ```
    async fn upsert_actor(
        &self,
        ctx: &Context<'_>,
        input: UpsertActorInput,
    ) -> GqlResult<ActorObject> {
        let data = ctx_data(ctx)?;
        data.db
            .upsert_actor(
                &input.id.0,
                &input.name,
                &input.system_prompt,
                &input.default_behavior_id.0,
                &input.status,
            )
            .await
            .map_err(map_anyhow)?;

        let actor = data
            .db
            .get_actor(&input.id.0)
            .await
            .map_err(map_anyhow)?
            .ok_or_else(|| gql_error_with_code("actor not found after upsert", "INTERNAL"))?;

        Ok(ActorObject::new(actor))
    }

    /// Deletes an actor by URI.
    ///
    /// Example:
    /// ```graphql
    /// mutation($id: Uri!) { deleteActor(id: $id) }
    /// ```
    async fn delete_actor(&self, ctx: &Context<'_>, id: UriScalar) -> GqlResult<bool> {
        let data = ctx_data(ctx)?;
        let deleted = data.db.delete_actor(&id.0).await.map_err(map_anyhow)?;
        Ok(deleted > 0)
    }

    /// Creates or updates a behavior.
    ///
    /// Example:
    /// ```graphql
    /// mutation($id: Uri!) {
    ///   upsertBehavior(input: {
    ///     id: $id
    ///     name: "default"
    ///     systemPrompt: "..."
    ///     sessionTurnConcurrency: "serial"
    ///     status: "ACTIVE"
    ///     requiredCapabilities: ["TaskGraph-listTasks"]
    ///   }) { id name status requiredCapabilities }
    /// }
    /// ```
    async fn upsert_behavior(
        &self,
        ctx: &Context<'_>,
        input: UpsertBehaviorInput,
    ) -> GqlResult<BehaviorObject> {
        let data = ctx_data(ctx)?;
        let required = serde_json::Value::Array(
            input
                .required_capabilities
                .into_iter()
                .map(serde_json::Value::String)
                .collect(),
        );

        data.db
            .upsert_behavior(
                &input.id.0,
                &input.name,
                &input.system_prompt,
                input.preferred_provider_id.as_deref(),
                &required,
                &input.session_turn_concurrency,
                &input.status,
            )
            .await
            .map_err(map_anyhow)?;

        let behavior = data
            .db
            .get_behavior(&input.id.0)
            .await
            .map_err(map_anyhow)?
            .ok_or_else(|| gql_error_with_code("behavior not found after upsert", "INTERNAL"))?;

        Ok(BehaviorObject::new(behavior))
    }

    /// Deletes a behavior by URI.
    ///
    /// Example:
    /// ```graphql
    /// mutation($id: Uri!) { deleteBehavior(id: $id) }
    /// ```
    async fn delete_behavior(&self, ctx: &Context<'_>, id: UriScalar) -> GqlResult<bool> {
        let data = ctx_data(ctx)?;
        let deleted = data.db.delete_behavior(&id.0).await.map_err(map_anyhow)?;
        Ok(deleted > 0)
    }

    /// Creates or updates a port.
    ///
    /// Usage notes:
    /// - `assignedActorId` is mirrored into `settings.actor_id` for compatibility.
    /// - `settings` must be a JSON object when provided.
    ///
    /// Example:
    /// ```graphql
    /// mutation {
    ///   upsertPort(input: {
    ///     name: "http"
    ///     provider: "custom"
    ///     enabled: true
    ///     allowsGuests: true
    ///   }) { id name enabled allowsGuests }
    /// }
    /// ```
    async fn upsert_port(
        &self,
        ctx: &Context<'_>,
        input: UpsertPortInput,
    ) -> GqlResult<PortObject> {
        let data = ctx_data(ctx)?;
        let mut settings = input
            .settings
            .map(|value| value.0)
            .unwrap_or_else(|| json!({}));
        if !settings.is_object() {
            return Err(gql_error_with_code(
                "settings must be a JSON object",
                "BAD_REQUEST",
            ));
        }

        if let Some(actor_id) = &input.assigned_actor_id {
            if let Some(object) = settings.as_object_mut() {
                object.insert(
                    "actor_id".to_string(),
                    serde_json::Value::String(actor_id.0.to_string()),
                );
            }
        } else if let Some(object) = settings.as_object_mut() {
            object.remove("actor_id");
        }

        data.db
            .upsert_port(
                &input.name,
                &input.provider,
                input.enabled,
                input.allows_guests,
                input.assigned_actor_id.as_ref().map(|uri| &uri.0),
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

    /// Creates or updates a port/session binding.
    ///
    /// Example:
    /// ```graphql
    /// mutation($session: Uri!, $key: Uri!) {
    ///   upsertPortBinding(input: {
    ///     portName: "telegram"
    ///     conversationKey: $key
    ///     sessionId: $session
    ///   }) { portName conversationKey sessionId }
    /// }
    /// ```
    async fn upsert_port_binding(
        &self,
        ctx: &Context<'_>,
        input: UpsertPortBindingInput,
    ) -> GqlResult<PortBindingObject> {
        let data = ctx_data(ctx)?;
        data.db
            .upsert_port_binding_record(
                &input.port_name,
                &input.conversation_key.0,
                &input.session_id.0,
            )
            .await
            .map_err(map_anyhow)?;

        Ok(PortBindingObject {
            port_name: input.port_name,
            conversation_key: input.conversation_key.0,
            session_id: input.session_id.0,
        })
    }

    /// Creates or updates a port/actor binding.
    ///
    /// Usage notes:
    /// - Pass `actorId: null` to clear the actor binding.
    ///
    /// Example:
    /// ```graphql
    /// mutation($key: Uri!, $actor: Uri!) {
    ///   upsertPortActorBinding(input: {
    ///     portName: "telegram"
    ///     conversationKey: $key
    ///     actorId: $actor
    ///   }) { portName conversationKey actorId }
    /// }
    /// ```
    async fn upsert_port_actor_binding(
        &self,
        ctx: &Context<'_>,
        input: UpsertPortActorBindingInput,
    ) -> GqlResult<PortActorBindingObject> {
        let data = ctx_data(ctx)?;
        if let Some(actor_id) = input.actor_id.as_ref() {
            data.db
                .upsert_port_actor_binding(&input.port_name, &input.conversation_key.0, &actor_id.0)
                .await
                .map_err(map_anyhow)?;
        } else {
            data.db
                .clear_port_actor_binding(&input.port_name, &input.conversation_key.0)
                .await
                .map_err(map_anyhow)?;
        }

        let actor_id = data
            .db
            .get_port_actor_binding(&input.port_name, &input.conversation_key.0)
            .await
            .map_err(map_anyhow)?;

        Ok(PortActorBindingObject {
            port_name: input.port_name,
            conversation_key: input.conversation_key.0,
            actor_id,
        })
    }

    /// Creates or updates a provider.
    ///
    /// Example:
    /// ```graphql
    /// mutation {
    ///   upsertProvider(input: {
    ///     provider: "openrouter"
    ///     providerKind: "openrouter"
    ///     apiKey: "sk-***"
    ///     enabled: true
    ///     defaultTextModel: "openai/gpt-4.1-mini"
    ///   }) { provider providerKind enabled }
    /// }
    /// ```
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

    /// Deletes a provider and associated usage summary.
    ///
    /// Example:
    /// ```graphql
    /// mutation { deleteProvider(provider: "openrouter") }
    /// ```
    async fn delete_provider(&self, ctx: &Context<'_>, provider: String) -> GqlResult<bool> {
        let data = ctx_data(ctx)?;
        let deleted = data
            .db
            .delete_provider(&provider)
            .await
            .map_err(map_anyhow)?;
        Ok(deleted > 0)
    }

    /// Creates or updates an app.
    ///
    /// Example:
    /// ```graphql
    /// mutation($id: Uri!) {
    ///   upsertApp(input: {
    ///     id: $id
    ///     name: "GitHub"
    ///     slug: "github"
    ///     description: "GitHub integration"
    ///     status: "ACTIVE"
    ///     builtIn: false
    ///     source: "custom"
    ///     authStrategy: "oauth2"
    ///     availableSecrets: ["GITHUB_TOKEN"]
    ///   }) { id slug status }
    /// }
    /// ```
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
                &input.status,
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

    /// Creates or updates an app capability.
    ///
    /// Example:
    /// ```graphql
    /// mutation($app: Uri!, $cap: Uri!) {
    ///   upsertAppCapability(input: {
    ///     appId: $app
    ///     capabilityId: $cap
    ///     name: "issues.list"
    ///     hint: "List GitHub issues"
    ///     mode: "READ"
    ///     instructions: "Use filters when possible"
    ///     status: "ACTIVE"
    ///   }) { id name status }
    /// }
    /// ```
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
                &input.status,
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

    /// Creates or updates an app connection.
    ///
    /// Example:
    /// ```graphql
    /// mutation($app: Uri!, $conn: Uri!, $owner: Uri) {
    ///   upsertAppConnection(input: {
    ///     appId: $app
    ///     connectionId: $conn
    ///     ownerUserId: $owner
    ///     status: "CONNECTED"
    ///   }) { id appId status }
    /// }
    /// ```
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
                &input.status,
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

    /// Creates or updates an app secret.
    ///
    /// Example:
    /// ```graphql
    /// mutation($app: Uri!, $secret: Uri!) {
    ///   upsertAppSecret(input: {
    ///     appId: $app
    ///     secretId: $secret
    ///     key: "GITHUB_TOKEN"
    ///     value: "..."
    ///     kind: "token"
    ///   }) { id key kind }
    /// }
    /// ```
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

    /// Creates or updates a session.
    ///
    /// Example:
    /// ```graphql
    /// mutation($id: Uri!, $user: Uri!, $port: Uri!) {
    ///   upsertSession(input: { sessionId: $id, users: [$user], port: $port }) { id users portId }
    /// }
    /// ```
    async fn upsert_session(
        &self,
        ctx: &Context<'_>,
        input: UpsertSessionInput,
    ) -> GqlResult<SessionObject> {
        let data = ctx_data(ctx)?;
        let users = input.users.into_iter().map(|uri| uri.0).collect::<Vec<_>>();
        data.db
            .upsert_session(&input.session_id.0, &users, &input.port.0)
            .await
            .map_err(map_anyhow)?;

        let session = data
            .db
            .get_session(&input.session_id.0)
            .await
            .map_err(map_anyhow)?
            .ok_or_else(|| gql_error_with_code("session not found after upsert", "INTERNAL"))?;

        Ok(SessionObject::new(session))
    }

    /// Appends a session message.
    ///
    /// Usage notes:
    /// - Prefer typed fields (`messageType`, `role`, `text`) over raw `payload`.
    ///
    /// Example:
    /// ```graphql
    /// mutation($session: Uri!) {
    ///   appendSessionMessage(input: {
    ///     sessionId: $session
    ///     messageType: "user"
    ///     role: "user"
    ///     text: "Hello"
    ///   }) { id messageIndex messageType role text }
    /// }
    /// ```
    async fn append_session_message(
        &self,
        ctx: &Context<'_>,
        input: AppendSessionMessageInput,
    ) -> GqlResult<SessionMessageObject> {
        let data = ctx_data(ctx)?;
        let payload = build_session_message_payload(&SessionMessageInput {
            message_type: input.message_type.clone(),
            role: input.role.clone(),
            text: input.text.clone(),
            payload: input.payload.clone(),
        })?;

        let index = data
            .db
            .append_session_message(&input.session_id.0, &payload)
            .await
            .map_err(map_anyhow)?;

        let message = data
            .db
            .get_session_message(&input.session_id.0, index)
            .await
            .map_err(map_anyhow)?
            .ok_or_else(|| gql_error_with_code("message not found after append", "INTERNAL"))?;

        Ok(SessionMessageObject::new(message))
    }

    /// Updates an existing session message.
    ///
    /// Example:
    /// ```graphql
    /// mutation($session: Uri!) {
    ///   patchSessionMessage(input: {
    ///     sessionId: $session
    ///     messageIndex: 0
    ///     message: { messageType: "user", role: "user", text: "Updated text" }
    ///   }) { id messageIndex text }
    /// }
    /// ```
    async fn patch_session_message(
        &self,
        ctx: &Context<'_>,
        input: PatchSessionMessageInput,
    ) -> GqlResult<SessionMessageObject> {
        let data = ctx_data(ctx)?;
        let payload = build_session_message_payload(&input.message)?;

        data.db
            .update_session_message(&input.session_id.0, input.message_index, &payload)
            .await
            .map_err(map_anyhow)?;

        let message = data
            .db
            .get_session_message(&input.session_id.0, input.message_index)
            .await
            .map_err(map_anyhow)?
            .ok_or_else(|| gql_error_with_code("session message not found", "NOT_FOUND"))?;

        Ok(SessionMessageObject::new(message))
    }

    /// Creates a clockwork job.
    ///
    /// Example:
    /// ```graphql
    /// mutation($actor: Uri!, $session: Uri!) {
    ///   createClockworkJob(input: {
    ///     jobId: "daily-standup"
    ///     kind: "cron"
    ///     actorId: $actor
    ///     sessionId: $session
    ///     messageType: "user"
    ///     payload: { text: "Daily standup" }
    ///     scheduleSpec: { cron: "0 9 * * 1-5" }
    ///   }) { id status nextRunAt }
    /// }
    /// ```
    async fn create_clockwork_job(
        &self,
        ctx: &Context<'_>,
        input: CreateClockworkJobInputGql,
    ) -> GqlResult<ClockworkJobObject> {
        let data = ctx_data(ctx)?;
        let create = CreateClockworkJobInput {
            job_id: input.job_id.clone(),
            kind: input.kind,
            target_actor_id: input.actor_id.0.to_string(),
            target_session_id: input.session_id.0.to_string(),
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
            .create_clockwork_job(&create)
            .await
            .map_err(map_anyhow)?;

        let job = data
            .db
            .get_clockwork_job(&input.job_id)
            .await
            .map_err(map_anyhow)?
            .ok_or_else(|| gql_error_with_code("clockwork job not found", "INTERNAL"))?;

        Ok(ClockworkJobObject::new(job))
    }

    /// Updates mutable clockwork job fields.
    ///
    /// Example:
    /// ```graphql
    /// mutation {
    ///   updateClockworkJob(
    ///     jobId: "daily-standup",
    ///     patch: { scheduleSpec: { cron: "0 10 * * 1-5" } }
    ///   ) { id status scheduleSpec }
    /// }
    /// ```
    async fn update_clockwork_job(
        &self,
        ctx: &Context<'_>,
        job_id: String,
        patch: UpdateClockworkJobInputGql,
    ) -> GqlResult<ClockworkJobObject> {
        let data = ctx_data(ctx)?;
        let update = UpdateClockworkJobInput {
            kind: patch.kind,
            target_actor_id: patch.actor_id.map(|uri| uri.0.to_string()),
            target_session_id: patch.session_id.map(|uri| uri.0.to_string()),
            message_type: patch.message_type,
            payload: patch.payload.map(|value| value.0),
            headers: patch.headers.map(|value| value.0),
            schedule_spec: patch.schedule_spec.map(|value| value.0),
            next_run_at: patch.next_run_at.map(Some),
        };

        data.db
            .update_clockwork_job(&job_id, &update)
            .await
            .map_err(map_anyhow)?;

        let job = data
            .db
            .get_clockwork_job(&job_id)
            .await
            .map_err(map_anyhow)?
            .ok_or_else(|| gql_error_with_code("clockwork job not found", "NOT_FOUND"))?;

        Ok(ClockworkJobObject::new(job))
    }

    /// Pauses an active clockwork job.
    ///
    /// Example:
    /// ```graphql
    /// mutation { pauseClockworkJob(jobId: "daily-standup") }
    /// ```
    async fn pause_clockwork_job(&self, ctx: &Context<'_>, job_id: String) -> GqlResult<bool> {
        let data = ctx_data(ctx)?;
        let updated = data
            .db
            .set_clockwork_job_status(&job_id, "paused")
            .await
            .map_err(map_anyhow)?;
        Ok(updated > 0)
    }

    /// Resumes a paused clockwork job.
    ///
    /// Example:
    /// ```graphql
    /// mutation { resumeClockworkJob(jobId: "daily-standup") }
    /// ```
    async fn resume_clockwork_job(&self, ctx: &Context<'_>, job_id: String) -> GqlResult<bool> {
        let data = ctx_data(ctx)?;
        let updated = data
            .db
            .set_clockwork_job_status(&job_id, "active")
            .await
            .map_err(map_anyhow)?;
        Ok(updated > 0)
    }

    /// Cancels a clockwork job.
    ///
    /// Example:
    /// ```graphql
    /// mutation { cancelClockworkJob(jobId: "daily-standup") }
    /// ```
    async fn cancel_clockwork_job(&self, ctx: &Context<'_>, job_id: String) -> GqlResult<bool> {
        let data = ctx_data(ctx)?;
        let updated = data
            .db
            .set_clockwork_job_status(&job_id, "cancelled")
            .await
            .map_err(map_anyhow)?;
        Ok(updated > 0)
    }

    /// Creates a task in the taskgraph store.
    ///
    /// Example:
    /// ```graphql
    /// mutation($session: Uri!, $creator: Uri!, $assignee: Uri!) {
    ///   createTask(input: {
    ///     sessionUri: $session
    ///     creatorAgentId: $creator
    ///     assigneeAgentId: $assignee
    ///     title: "Ship borg-gql docs"
    ///     description: "Document all schema entrypoints"
    ///   }) { id title status }
    /// }
    /// ```
    async fn create_task(
        &self,
        ctx: &Context<'_>,
        input: CreateTaskInputGql,
    ) -> GqlResult<TaskObject> {
        let data = ctx_data(ctx)?;
        let store = TaskGraphStore::new(data.db.clone());

        let created = store
            .create_task(
                input.session_uri.0.as_str(),
                input.creator_agent_id.0.as_str(),
                CreateTaskInput {
                    title: input.title,
                    description: input.description,
                    definition_of_done: input.definition_of_done,
                    assignee_agent_id: input.assignee_agent_id.0.to_string(),
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

    /// Updates mutable task text fields.
    ///
    /// Example:
    /// ```graphql
    /// mutation($task: Uri!, $session: Uri!) {
    ///   updateTask(input: {
    ///     taskId: $task
    ///     sessionUri: $session
    ///     title: "Updated title"
    ///   }) { id title updatedAt }
    /// }
    /// ```
    async fn update_task(
        &self,
        ctx: &Context<'_>,
        input: UpdateTaskInputGql,
    ) -> GqlResult<TaskObject> {
        let data = ctx_data(ctx)?;
        let store = TaskGraphStore::new(data.db.clone());
        let task = store
            .update_task_fields(
                input.session_uri.0.as_str(),
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

    /// Moves a task to a new allowed status.
    ///
    /// Usage notes:
    /// - Auth/session constraints follow taskgraph rules (assignee/reviewer checks).
    ///
    /// Example:
    /// ```graphql
    /// mutation($task: Uri!, $session: Uri!) {
    ///   setTaskStatus(input: { taskId: $task, sessionUri: $session, status: DOING }) {
    ///     id
    ///     status
    ///   }
    /// }
    /// ```
    async fn set_task_status(
        &self,
        ctx: &Context<'_>,
        input: SetTaskStatusInput,
    ) -> GqlResult<TaskObject> {
        let data = ctx_data(ctx)?;
        let store = TaskGraphStore::new(data.db.clone());
        let task = store
            .set_task_status(
                input.session_uri.0.as_str(),
                input.task_id.0.as_str(),
                input.status.into(),
            )
            .await
            .map_err(map_anyhow)?;

        TaskObject::try_new(task)
    }

    /// Placeholder runtime wrapper; enabled after `borg-api` integration.
    ///
    /// Usage notes:
    /// - Currently returns `BAD_REQUEST`.
    /// - Keep frontend contracts ready for upcoming runtime integration.
    ///
    /// Example:
    /// ```graphql
    /// mutation($actor: Uri!, $session: Uri!, $user: Uri!) {
    ///   runActorChat(input: {
    ///     actorId: $actor
    ///     sessionId: $session
    ///     userId: $user
    ///     text: "Summarize pending tasks"
    ///   }) { ok message }
    /// }
    /// ```
    async fn run_actor_chat(&self, _input: RunActorChatInput) -> GqlResult<RunActorChatResult> {
        Err(gql_error_with_code(
            "runActorChat is not available in standalone borg-gql",
            "BAD_REQUEST",
        ))
    }

    /// Placeholder runtime wrapper; enabled after `borg-api` integration.
    ///
    /// Usage notes:
    /// - Currently returns `BAD_REQUEST`.
    /// - Expected to mirror `POST /ports/http` behavior in a later phase.
    ///
    /// Example:
    /// ```graphql
    /// mutation($user: Uri!) {
    ///   runPortHttp(input: { userId: $user, text: "Hello" }) { ok message }
    /// }
    /// ```
    async fn run_port_http(&self, _input: RunPortHttpInput) -> GqlResult<RunPortHttpResult> {
        Err(gql_error_with_code(
            "runPortHttp is not available in standalone borg-gql",
            "BAD_REQUEST",
        ))
    }
}
