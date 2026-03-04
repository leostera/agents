use super::super::*;

#[Object(use_type_description)]
impl QueryRoot {
    /// Fetches a single graph node by URI and resolves the concrete runtime type.
    ///
    /// Usage notes:
    /// - Works for actor/behavior/session/port/provider/app/task/policy/user/memory entities.
    /// - Use inline fragments to read type-specific fields.
    ///
    /// Example:
    /// ```graphql
    /// query($id: Uri!) {
    ///   node(id: $id) {
    ///     id
    ///     ... on Actor { name status }
    ///   }
    /// }
    /// ```
    async fn node(&self, ctx: &Context<'_>, id: UriScalar) -> GqlResult<Option<Node>> {
        let data = ctx_data(ctx)?;

        match parse_uri_kind(&id.0) {
            Some("actor") => Ok(data
                .db
                .get_actor(&id.0)
                .await
                .map_err(map_anyhow)?
                .map(ActorObject::new)
                .map(Node::from)),
            Some("behavior") => Ok(data
                .db
                .get_behavior(&id.0)
                .await
                .map_err(map_anyhow)?
                .map(BehaviorObject::new)
                .map(Node::from)),
            Some("session") => Ok(data
                .db
                .get_session(&id.0)
                .await
                .map_err(map_anyhow)?
                .map(SessionObject::new)
                .map(Node::from)),
            Some("port") => Ok(data
                .db
                .get_port_by_id(&id.0)
                .await
                .map_err(map_anyhow)?
                .map(PortObject::new)
                .map(Node::from)),
            Some("app") => Ok(data
                .db
                .get_app(&id.0)
                .await
                .map_err(map_anyhow)?
                .map(AppObject::new)
                .map(Node::from)),
            Some("policy") => Ok(data
                .db
                .get_policy(&id.0)
                .await
                .map_err(map_anyhow)?
                .map(PolicyObject::new)
                .map(Node::from)),
            Some("user") => Ok(data
                .db
                .get_user(&id.0)
                .await
                .map_err(map_anyhow)?
                .map(UserObject::new)
                .map(Node::from)),
            Some("task") => {
                let store = TaskGraphStore::new(data.db.clone());
                match store.get_task(id.0.as_str()).await {
                    Ok(task) => Ok(Some(Node::from(TaskObject::try_new(task)?))),
                    Err(err) if err.to_string().contains("not_found") => Ok(None),
                    Err(err) => Err(map_anyhow(err)),
                }
            }
            Some("provider") => {
                let Some(provider_key) = parse_uri_id(&id.0) else {
                    return Ok(None);
                };
                Ok(data
                    .db
                    .get_provider(provider_key)
                    .await
                    .map_err(map_anyhow)?
                    .map(ProviderObject::try_new)
                    .transpose()?
                    .map(Node::from))
            }
            _ => {
                let memory_uri = to_memory_uri(&id.0)?;
                Ok(data
                    .memory
                    .get_entity_uri(&memory_uri)
                    .await
                    .map_err(map_anyhow)?
                    .map(MemoryEntityObject::new)
                    .map(Node::from))
            }
        }
    }

    /// Fetches one actor by URI.
    ///
    /// Example:
    /// ```graphql
    /// query($id: Uri!) { actor(id: $id) { id name status } }
    /// ```
    async fn actor(&self, ctx: &Context<'_>, id: UriScalar) -> GqlResult<Option<ActorObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_actor(&id.0)
            .await
            .map_err(map_anyhow)?
            .map(ActorObject::new))
    }

    /// Lists actors ordered by most-recent update.
    ///
    /// Usage notes:
    /// - `first` defaults to 25 and is capped server-side.
    /// - Pass the previous `endCursor` into `after` to paginate.
    ///
    /// Example:
    /// ```graphql
    /// query {
    ///   actors(first: 10) {
    ///     edges { cursor node { id name } }
    ///     pageInfo { hasNextPage endCursor }
    ///   }
    /// }
    /// ```
    async fn actors(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<ActorConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let actors = data.db.list_actors(fetch_limit).await.map_err(map_anyhow)?;
        let (page, has_next_page) = apply_offset_pagination(actors, start, first);

        let edges = page
            .into_iter()
            .map(|(index, record)| ActorEdge {
                cursor: encode_offset_cursor(index),
                node: ActorObject::new(record),
            })
            .collect::<Vec<_>>();

        Ok(ActorConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }

    /// Fetches one behavior by URI.
    ///
    /// Example:
    /// ```graphql
    /// query($id: Uri!) { behavior(id: $id) { id name preferredProviderId } }
    /// ```
    async fn behavior(
        &self,
        ctx: &Context<'_>,
        id: UriScalar,
    ) -> GqlResult<Option<BehaviorObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_behavior(&id.0)
            .await
            .map_err(map_anyhow)?
            .map(BehaviorObject::new))
    }

    /// Lists behaviors ordered by most-recent update.
    ///
    /// Example:
    /// ```graphql
    /// query {
    ///   behaviors(first: 20) {
    ///     edges { node { id name status requiredCapabilities } }
    ///   }
    /// }
    /// ```
    async fn behaviors(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<BehaviorConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let behaviors = data
            .db
            .list_behaviors(fetch_limit)
            .await
            .map_err(map_anyhow)?;
        let (page, has_next_page) = apply_offset_pagination(behaviors, start, first);

        let edges = page
            .into_iter()
            .map(|(index, record)| BehaviorEdge {
                cursor: encode_offset_cursor(index),
                node: BehaviorObject::new(record),
            })
            .collect::<Vec<_>>();

        Ok(BehaviorConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }

    /// Fetches one session by URI.
    ///
    /// Example:
    /// ```graphql
    /// query($id: Uri!) { session(id: $id) { id users portId updatedAt } }
    /// ```
    async fn session(&self, ctx: &Context<'_>, id: UriScalar) -> GqlResult<Option<SessionObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_session(&id.0)
            .await
            .map_err(map_anyhow)?
            .map(SessionObject::new))
    }

    /// Lists sessions ordered by most-recent update.
    ///
    /// Usage notes:
    /// - Optional filters: `portId` and `userId`.
    /// - Use nested `messages` for chat timeline reads.
    ///
    /// Example:
    /// ```graphql
    /// query($port: Uri!) {
    ///   sessions(first: 10, portId: $port) {
    ///     edges { node { id updatedAt } }
    ///   }
    /// }
    /// ```
    async fn sessions(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
        port_id: Option<UriScalar>,
        user_id: Option<UriScalar>,
    ) -> GqlResult<SessionConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let sessions = data
            .db
            .list_sessions(
                fetch_limit,
                port_id.as_ref().map(|uri| &uri.0),
                user_id.as_ref().map(|uri| &uri.0),
            )
            .await
            .map_err(map_anyhow)?;

        let (page, has_next_page) = apply_offset_pagination(sessions, start, first);

        let edges = page
            .into_iter()
            .map(|(index, record)| SessionEdge {
                cursor: encode_offset_cursor(index),
                node: SessionObject::new(record),
            })
            .collect::<Vec<_>>();

        Ok(SessionConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }

    /// Fetches one port by canonical port name (for example `http`, `telegram`).
    ///
    /// Example:
    /// ```graphql
    /// query { port(name: "http") { id name provider enabled } }
    /// ```
    async fn port(&self, ctx: &Context<'_>, name: String) -> GqlResult<Option<PortObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_port(&name)
            .await
            .map_err(map_anyhow)?
            .map(PortObject::new))
    }

    /// Fetches one port by URI.
    ///
    /// Example:
    /// ```graphql
    /// query($id: Uri!) { portById(id: $id) { id name allowsGuests } }
    /// ```
    async fn port_by_id(&self, ctx: &Context<'_>, id: UriScalar) -> GqlResult<Option<PortObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_port_by_id(&id.0)
            .await
            .map_err(map_anyhow)?
            .map(PortObject::new))
    }

    /// Lists ports ordered by activity.
    ///
    /// Usage notes:
    /// - Includes `activeSessions` and binding relations for routing debugging.
    ///
    /// Example:
    /// ```graphql
    /// query {
    ///   ports(first: 20) {
    ///     edges { node { name provider activeSessions } }
    ///   }
    /// }
    /// ```
    async fn ports(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<PortConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let ports = data.db.list_ports(fetch_limit).await.map_err(map_anyhow)?;
        let (page, has_next_page) = apply_offset_pagination(ports, start, first);

        let edges = page
            .into_iter()
            .map(|(index, record)| PortEdge {
                cursor: encode_offset_cursor(index),
                node: PortObject::new(record),
            })
            .collect::<Vec<_>>();

        Ok(PortConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }

    /// Fetches one provider by provider key.
    ///
    /// Example:
    /// ```graphql
    /// query { provider(provider: "openai") { id provider providerKind enabled } }
    /// ```
    async fn provider(
        &self,
        ctx: &Context<'_>,
        provider: String,
    ) -> GqlResult<Option<ProviderObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_provider(&provider)
            .await
            .map_err(map_anyhow)?
            .map(ProviderObject::try_new)
            .transpose()?)
    }

    /// Lists configured model providers.
    ///
    /// Example:
    /// ```graphql
    /// query {
    ///   providers(first: 10) {
    ///     edges { node { provider providerKind defaultTextModel tokensUsed } }
    ///   }
    /// }
    /// ```
    async fn providers(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<ProviderConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let providers = data
            .db
            .list_providers(fetch_limit)
            .await
            .map_err(map_anyhow)?;
        let (page, has_next_page) = apply_offset_pagination(providers, start, first);

        let mut edges = Vec::with_capacity(page.len());
        for (index, record) in page {
            edges.push(ProviderEdge {
                cursor: encode_offset_cursor(index),
                node: ProviderObject::try_new(record)?,
            });
        }

        Ok(ProviderConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }

    /// Fetches one app by URI.
    ///
    /// Example:
    /// ```graphql
    /// query($id: Uri!) { app(id: $id) { id name slug status } }
    /// ```
    async fn app(&self, ctx: &Context<'_>, id: UriScalar) -> GqlResult<Option<AppObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_app(&id.0)
            .await
            .map_err(map_anyhow)?
            .map(AppObject::new))
    }

    /// Fetches one app by slug.
    ///
    /// Example:
    /// ```graphql
    /// query { appBySlug(slug: "github") { id name capabilities(first: 5) { edges { node { name } } } } }
    /// ```
    async fn app_by_slug(&self, ctx: &Context<'_>, slug: String) -> GqlResult<Option<AppObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_app_by_slug(&slug)
            .await
            .map_err(map_anyhow)?
            .map(AppObject::new))
    }

    /// Lists apps available in Borg.
    ///
    /// Example:
    /// ```graphql
    /// query {
    ///   apps(first: 25) {
    ///     edges { node { id slug authStrategy availableSecrets } }
    ///   }
    /// }
    /// ```
    async fn apps(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<AppListConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let apps = data.db.list_apps(fetch_limit).await.map_err(map_anyhow)?;
        let (page, has_next_page) = apply_offset_pagination(apps, start, first);

        let edges = page
            .into_iter()
            .map(|(index, record)| AppEdge {
                cursor: encode_offset_cursor(index),
                node: AppObject::new(record),
            })
            .collect::<Vec<_>>();

        Ok(AppListConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }

    /// Fetches one clockwork job by `jobId`.
    ///
    /// Example:
    /// ```graphql
    /// query { clockworkJob(jobId: "daily-digest") { id status nextRunAt } }
    /// ```
    async fn clockwork_job(
        &self,
        ctx: &Context<'_>,
        job_id: String,
    ) -> GqlResult<Option<ClockworkJobObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_clockwork_job(&job_id)
            .await
            .map_err(map_anyhow)?
            .map(ClockworkJobObject::new))
    }

    /// Lists clockwork jobs with optional status filtering.
    ///
    /// Example:
    /// ```graphql
    /// query {
    ///   clockworkJobs(first: 20, status: "active") {
    ///     edges { node { id kind status runs(first: 5) { edges { node { id firedAt } } } } }
    ///   }
    /// }
    /// ```
    async fn clockwork_jobs(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
        status: Option<String>,
    ) -> GqlResult<ClockworkJobConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let jobs = data
            .db
            .list_clockwork_jobs(fetch_limit, status.as_deref())
            .await
            .map_err(map_anyhow)?;
        let (page, has_next_page) = apply_offset_pagination(jobs, start, first);

        let edges = page
            .into_iter()
            .map(|(index, record)| ClockworkJobEdge {
                cursor: encode_offset_cursor(index),
                node: ClockworkJobObject::new(record),
            })
            .collect::<Vec<_>>();

        Ok(ClockworkJobConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }

    /// Fetches one task by URI.
    ///
    /// Example:
    /// ```graphql
    /// query($id: Uri!) {
    ///   task(id: $id) {
    ///     id title status
    ///     comments(first: 10) { edges { node { id body } } }
    ///   }
    /// }
    /// ```
    async fn task(&self, ctx: &Context<'_>, id: UriScalar) -> GqlResult<Option<TaskObject>> {
        let data = ctx_data(ctx)?;
        let store = TaskGraphStore::new(data.db.clone());
        match store.get_task(id.0.as_str()).await {
            Ok(task) => Ok(Some(TaskObject::try_new(task)?)),
            Err(err) if err.to_string().contains("not_found") => Ok(None),
            Err(err) => Err(map_anyhow(err)),
        }
    }

    /// Lists top-level taskgraph tasks.
    ///
    /// Usage notes:
    /// - Cursor format follows taskgraph ordering (`createdAt`, `id`).
    /// - Traverse children via `Task.children`.
    ///
    /// Example:
    /// ```graphql
    /// query {
    ///   tasks(first: 15) {
    ///     edges { node { id title status parentUri } }
    ///   }
    /// }
    /// ```
    async fn tasks(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<TaskConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let store = TaskGraphStore::new(data.db.clone());

        let (tasks, next_cursor) = store
            .list_tasks(ListParams {
                cursor: after,
                limit: first,
            })
            .await
            .map_err(map_anyhow)?;

        let mut edges = Vec::with_capacity(tasks.len());
        for task in tasks {
            let cursor = encode_task_cursor(&task.created_at, &task.uri);
            edges.push(TaskEdge {
                cursor,
                node: TaskObject::try_new(task)?,
            });
        }

        Ok(TaskConnection {
            page_info: PageInfo {
                has_next_page: next_cursor.is_some(),
                end_cursor: edges.last().map(|edge| edge.cursor.clone()).or(next_cursor),
            },
            edges,
        })
    }

    /// Fetches one memory entity by URI.
    ///
    /// Example:
    /// ```graphql
    /// query($id: Uri!) { memoryEntity(id: $id) { id label props { key value { kind text } } } }
    /// ```
    async fn memory_entity(
        &self,
        ctx: &Context<'_>,
        id: UriScalar,
    ) -> GqlResult<Option<MemoryEntityObject>> {
        let data = ctx_data(ctx)?;
        let memory_uri = to_memory_uri(&id.0)?;
        Ok(data
            .memory
            .get_entity_uri(&memory_uri)
            .await
            .map_err(map_anyhow)?
            .map(MemoryEntityObject::new))
    }

    /// Searches memory entities by free text plus optional namespace/kind filters.
    ///
    /// Example:
    /// ```graphql
    /// query {
    ///   memoryEntities(queryText: "alice", kind: "person", first: 10) {
    ///     edges { node { id label } }
    ///   }
    /// }
    /// ```
    async fn memory_entities(
        &self,
        ctx: &Context<'_>,
        query_text: String,
        ns: Option<String>,
        kind: Option<String>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<MemoryEntityConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let results = data
            .memory
            .search_query(SearchQuery {
                ns,
                kind,
                name: None,
                query_text: Some(query_text),
                limit: Some(fetch_limit),
            })
            .await
            .map_err(map_anyhow)?;

        let (page, has_next_page) = apply_offset_pagination(results.entities, start, first);
        let edges = page
            .into_iter()
            .map(|(index, entity)| MemoryEntityEdge {
                cursor: encode_offset_cursor(index),
                node: MemoryEntityObject::new(entity),
            })
            .collect::<Vec<_>>();

        Ok(MemoryEntityConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }

    /// Fetches one fact row by fact URI string.
    ///
    /// Example:
    /// ```graphql
    /// query($id: String!) { memoryFact(id: $id) { id arity value { kind text } } }
    /// ```
    async fn memory_fact(
        &self,
        ctx: &Context<'_>,
        id: String,
    ) -> GqlResult<Option<MemoryFactObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .memory
            .get_fact(&id)
            .await
            .map_err(map_anyhow)?
            .map(MemoryFactObject::new))
    }

    /// Lists fact rows with optional entity/field filters.
    ///
    /// Usage notes:
    /// - Set `includeRetracted: true` for audit/replay tooling.
    ///
    /// Example:
    /// ```graphql
    /// query($entity: Uri!) {
    ///   memoryFacts(entityId: $entity, first: 20) {
    ///     edges { node { id field value { kind text reference } } }
    ///   }
    /// }
    /// ```
    async fn memory_facts(
        &self,
        ctx: &Context<'_>,
        entity_id: Option<UriScalar>,
        field_id: Option<UriScalar>,
        include_retracted: Option<bool>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<MemoryFactConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let entity = entity_id
            .as_ref()
            .map(|uri| to_memory_uri(&uri.0))
            .transpose()?;
        let field = field_id
            .as_ref()
            .map(|uri| to_memory_uri(&uri.0))
            .transpose()?;

        let facts = data
            .memory
            .list_facts(
                entity.as_ref(),
                field.as_ref(),
                include_retracted.unwrap_or(false),
                fetch_limit,
            )
            .await
            .map_err(map_anyhow)?;

        let (page, has_next_page) = apply_offset_pagination(facts, start, first);
        let edges = page
            .into_iter()
            .map(|(index, fact)| MemoryFactEdge {
                cursor: encode_offset_cursor(index),
                node: MemoryFactObject::new(fact),
            })
            .collect::<Vec<_>>();

        Ok(MemoryFactConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }

    /// Fetches one policy by URI.
    ///
    /// Example:
    /// ```graphql
    /// query($id: Uri!) { policy(id: $id) { id uses(first: 10) { edges { node { entityId } } } } }
    /// ```
    async fn policy(&self, ctx: &Context<'_>, id: UriScalar) -> GqlResult<Option<PolicyObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_policy(&id.0)
            .await
            .map_err(map_anyhow)?
            .map(PolicyObject::new))
    }

    /// Lists policies.
    ///
    /// Example:
    /// ```graphql
    /// query { policies(first: 25) { edges { node { id updatedAt } } } }
    /// ```
    async fn policies(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<PolicyConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let policies = data
            .db
            .list_policies(fetch_limit)
            .await
            .map_err(map_anyhow)?;
        let (page, has_next_page) = apply_offset_pagination(policies, start, first);

        let edges = page
            .into_iter()
            .map(|(index, record)| PolicyEdge {
                cursor: encode_offset_cursor(index),
                node: PolicyObject::new(record),
            })
            .collect::<Vec<_>>();

        Ok(PolicyConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }

    /// Fetches one user by URI.
    ///
    /// Example:
    /// ```graphql
    /// query($id: Uri!) { user(id: $id) { id createdAt updatedAt } }
    /// ```
    async fn user(&self, ctx: &Context<'_>, id: UriScalar) -> GqlResult<Option<UserObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_user(&id.0)
            .await
            .map_err(map_anyhow)?
            .map(UserObject::new))
    }

    /// Lists users.
    ///
    /// Example:
    /// ```graphql
    /// query { users(first: 50) { edges { node { id updatedAt } } } }
    /// ```
    async fn users(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
    ) -> GqlResult<UserConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let users = data.db.list_users(fetch_limit).await.map_err(map_anyhow)?;
        let (page, has_next_page) = apply_offset_pagination(users, start, first);

        let edges = page
            .into_iter()
            .map(|(index, record)| UserEdge {
                cursor: encode_offset_cursor(index),
                node: UserObject::new(record),
            })
            .collect::<Vec<_>>();

        Ok(UserConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }
}
