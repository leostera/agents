use super::super::*;

#[Object]
impl QueryRoot {
    async fn version(&self) -> &str {
        "0.1.0"
    }

    async fn node(&self, ctx: &Context<'_>, id: UriScalar) -> GqlResult<Option<Node>> {
        let data = ctx_data(ctx)?;

        match parse_uri_kind(&id.0) {
            Some("actor") => Ok(data
                .db
                .get_actor(&borg_core::ActorId(id.0.clone()))
                .await
                .map_err(map_anyhow)?
                .map(ActorObject::new)
                .map(Node::from)),
            Some("port") => Ok(data
                .db
                .get_port_by_id(&borg_core::PortId(id.0.clone()))
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

    async fn actor(&self, ctx: &Context<'_>, id: UriScalar) -> GqlResult<Option<ActorObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_actor(&borg_core::ActorId(id.0))
            .await
            .map_err(map_anyhow)?
            .map(ActorObject::new))
    }

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

    async fn port(&self, ctx: &Context<'_>, name: String) -> GqlResult<Option<PortObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_port(&name)
            .await
            .map_err(map_anyhow)?
            .map(PortObject::new))
    }

    async fn port_by_id(&self, ctx: &Context<'_>, id: UriScalar) -> GqlResult<Option<PortObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_port_by_id(&borg_core::PortId(id.0))
            .await
            .map_err(map_anyhow)?
            .map(PortObject::new))
    }

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

    async fn app(&self, ctx: &Context<'_>, id: UriScalar) -> GqlResult<Option<AppObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_app(&id.0)
            .await
            .map_err(map_anyhow)?
            .map(AppObject::new))
    }

    async fn app_by_slug(&self, ctx: &Context<'_>, slug: String) -> GqlResult<Option<AppObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_app_by_slug(&slug)
            .await
            .map_err(map_anyhow)?
            .map(AppObject::new))
    }

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

    async fn schedule_job(
        &self,
        ctx: &Context<'_>,
        job_id: String,
    ) -> GqlResult<Option<ScheduleJobObject>> {
        let data = ctx_data(ctx)?;
        Ok(data
            .db
            .get_schedule_job(&job_id)
            .await
            .map_err(map_anyhow)?
            .map(ScheduleJobObject::new))
    }

    async fn schedule_jobs(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
        status: Option<ScheduleJobStatusValue>,
    ) -> GqlResult<ScheduleJobConnection> {
        let data = ctx_data(ctx)?;
        let first = data.normalize_first(first)?;
        let start = decode_offset_cursor(after.as_deref())?;
        let fetch_limit = start + first + 1;

        let jobs = data
            .db
            .list_schedule_jobs(fetch_limit, status.map(ScheduleJobStatusValue::as_db_str))
            .await
            .map_err(map_anyhow)?;
        let (page, has_next_page) = apply_offset_pagination(jobs, start, first);

        let edges = page
            .into_iter()
            .map(|(index, record)| ScheduleJobEdge {
                cursor: encode_offset_cursor(index),
                node: ScheduleJobObject::new(record),
            })
            .collect::<Vec<_>>();

        Ok(ScheduleJobConnection {
            page_info: PageInfo {
                has_next_page,
                end_cursor: edges.last().map(|edge| edge.cursor.clone()),
            },
            edges,
        })
    }

    async fn task(&self, ctx: &Context<'_>, id: UriScalar) -> GqlResult<Option<TaskObject>> {
        let data = ctx_data(ctx)?;
        let store = TaskGraphStore::new(data.db.clone());
        match store.get_task(id.0.as_str()).await {
            Ok(task) => Ok(Some(TaskObject::try_new(task)?)),
            Err(err) if err.to_string().contains("not_found") => Ok(None),
            Err(err) => Err(map_anyhow(err)),
        }
    }

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
}
