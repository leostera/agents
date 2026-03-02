use anyhow::Result;
use borg_core::Uri;
use chrono::Utc;
use serde_json::json;
use sqlx::Row;

use crate::utils::parse_ts;
use crate::{AppCapabilityRecord, AppConnectionRecord, AppRecord, AppSecretRecord, BorgDb};

impl BorgDb {
    pub async fn list_apps(&self, limit: usize) -> Result<Vec<AppRecord>> {
        let limit = i64::try_from(limit).unwrap_or(100);
        let rows = sqlx::query!(
            r#"SELECT
                app_id as "app_id!: String",
                name as "name!: String",
                slug as "slug!: String",
                description as "description!: String",
                status as "status!: String",
                built_in as "built_in!: i64",
                source as "source!: String",
                auth_strategy as "auth_strategy!: String",
                auth_config_json as "auth_config_json!: String",
                available_secrets as "available_secrets!: String",
                created_at as "created_at!: String",
                updated_at as "updated_at!: String"
            FROM apps
            ORDER BY updated_at DESC, slug ASC
            LIMIT ?1"#,
            limit,
        )
        .fetch_all(self.conn.pool())
        .await?;

        rows.into_iter()
            .map(|row| {
                Ok(AppRecord {
                    app_id: Uri::parse(&row.app_id)?,
                    name: row.name,
                    slug: row.slug,
                    description: row.description,
                    status: row.status,
                    built_in: row.built_in != 0,
                    source: row.source,
                    auth_strategy: row.auth_strategy,
                    auth_config_json: serde_json::from_str(&row.auth_config_json)
                        .unwrap_or_else(|_| json!({})),
                    available_secrets: serde_json::from_str(&row.available_secrets)
                        .unwrap_or_default(),
                    created_at: parse_ts(&row.created_at)?,
                    updated_at: parse_ts(&row.updated_at)?,
                })
            })
            .collect()
    }

    pub async fn get_app(&self, app_id: &Uri) -> Result<Option<AppRecord>> {
        let app_id = app_id.to_string();
        let row = sqlx::query!(
            r#"SELECT
                app_id as "app_id!: String",
                name as "name!: String",
                slug as "slug!: String",
                description as "description!: String",
                status as "status!: String",
                built_in as "built_in!: i64",
                source as "source!: String",
                auth_strategy as "auth_strategy!: String",
                auth_config_json as "auth_config_json!: String",
                available_secrets as "available_secrets!: String",
                created_at as "created_at!: String",
                updated_at as "updated_at!: String"
            FROM apps
            WHERE app_id = ?1
            LIMIT 1"#,
            app_id,
        )
        .fetch_optional(self.conn.pool())
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };

        Ok(Some(AppRecord {
            app_id: Uri::parse(&row.app_id)?,
            name: row.name,
            slug: row.slug,
            description: row.description,
            status: row.status,
            built_in: row.built_in != 0,
            source: row.source,
            auth_strategy: row.auth_strategy,
            auth_config_json: serde_json::from_str(&row.auth_config_json)
                .unwrap_or_else(|_| json!({})),
            available_secrets: serde_json::from_str(&row.available_secrets).unwrap_or_default(),
            created_at: parse_ts(&row.created_at)?,
            updated_at: parse_ts(&row.updated_at)?,
        }))
    }

    pub async fn get_app_by_slug(&self, slug: &str) -> Result<Option<AppRecord>> {
        let row = sqlx::query!(
            r#"SELECT
                app_id as "app_id!: String",
                name as "name!: String",
                slug as "slug!: String",
                description as "description!: String",
                status as "status!: String",
                built_in as "built_in!: i64",
                source as "source!: String",
                auth_strategy as "auth_strategy!: String",
                auth_config_json as "auth_config_json!: String",
                available_secrets as "available_secrets!: String",
                created_at as "created_at!: String",
                updated_at as "updated_at!: String"
            FROM apps
            WHERE slug = ?1
            LIMIT 1"#,
            slug,
        )
        .fetch_optional(self.conn.pool())
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };

        Ok(Some(AppRecord {
            app_id: Uri::parse(&row.app_id)?,
            name: row.name,
            slug: row.slug,
            description: row.description,
            status: row.status,
            built_in: row.built_in != 0,
            source: row.source,
            auth_strategy: row.auth_strategy,
            auth_config_json: serde_json::from_str(&row.auth_config_json)
                .unwrap_or_else(|_| json!({})),
            available_secrets: serde_json::from_str(&row.available_secrets).unwrap_or_default(),
            created_at: parse_ts(&row.created_at)?,
            updated_at: parse_ts(&row.updated_at)?,
        }))
    }

    pub async fn upsert_app(
        &self,
        app_id: &Uri,
        name: &str,
        slug: &str,
        description: &str,
        status: &str,
    ) -> Result<()> {
        self.upsert_app_with_metadata(
            app_id,
            name,
            slug,
            description,
            status,
            false,
            "custom",
            "none",
            &json!({}),
            &[],
        )
        .await
    }

    pub async fn upsert_builtin_app(
        &self,
        app_id: &Uri,
        name: &str,
        slug: &str,
        description: &str,
        status: &str,
    ) -> Result<()> {
        self.upsert_app_with_metadata(
            app_id,
            name,
            slug,
            description,
            status,
            true,
            "managed",
            "none",
            &json!({}),
            &[],
        )
        .await
    }

    pub async fn upsert_app_with_metadata(
        &self,
        app_id: &Uri,
        name: &str,
        slug: &str,
        description: &str,
        status: &str,
        built_in: bool,
        source: &str,
        auth_strategy: &str,
        auth_config_json: &serde_json::Value,
        available_secrets: &[String],
    ) -> Result<()> {
        let app_id = app_id.to_string();
        let name = name.to_string();
        let slug = slug.to_string();
        let description = description.to_string();
        let status = status.to_string();
        let source = source.to_string();
        let auth_strategy = auth_strategy.to_string();
        let auth_config_json = auth_config_json.to_string();
        let available_secrets = serde_json::to_string(available_secrets)?;
        let now = Utc::now().to_rfc3339();
        let created_at = now.clone();
        let updated_at = now;
        let built_in_i64 = if built_in { 1_i64 } else { 0_i64 };
        sqlx::query!(
            r#"
            INSERT INTO apps(
              app_id, name, slug, description, status, built_in, source, auth_strategy, auth_config_json, available_secrets, created_at, updated_at
            )
            VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(app_id) DO UPDATE SET
              name = excluded.name,
              slug = excluded.slug,
              description = excluded.description,
              status = excluded.status,
              source = excluded.source,
              auth_strategy = excluded.auth_strategy,
              auth_config_json = excluded.auth_config_json,
              available_secrets = excluded.available_secrets,
              updated_at = excluded.updated_at
            "#,
            app_id,
            name,
            slug,
            description,
            status,
            built_in_i64,
            source,
            auth_strategy,
            auth_config_json,
            available_secrets,
            created_at,
            updated_at,
        )
        .execute(self.conn.pool())
        .await?;
        Ok(())
    }

    pub async fn delete_app(&self, app_id: &Uri) -> Result<u64> {
        let app_id = app_id.to_string();
        let deleted = sqlx::query!("DELETE FROM apps WHERE app_id = ?1", app_id)
            .execute(self.conn.pool())
            .await?
            .rows_affected();
        Ok(deleted)
    }

    pub async fn list_app_connections(
        &self,
        app_id: &Uri,
        limit: usize,
    ) -> Result<Vec<AppConnectionRecord>> {
        let limit = i64::try_from(limit).unwrap_or(100);
        let app_id = app_id.to_string();
        let rows = sqlx::query(
            r#"SELECT
                connection_id,
                app_id,
                owner_user_id,
                provider_account_id,
                external_user_id,
                status,
                connection_json,
                created_at,
                updated_at
              FROM app_connections
              WHERE app_id = ?1
              ORDER BY updated_at DESC, connection_id ASC
              LIMIT ?2"#,
        )
        .bind(app_id)
        .bind(limit)
        .fetch_all(self.conn.pool())
        .await?;

        rows.into_iter().map(map_app_connection_row).collect()
    }

    pub async fn get_app_connection(
        &self,
        app_id: &Uri,
        connection_id: &Uri,
    ) -> Result<Option<AppConnectionRecord>> {
        let row = sqlx::query(
            r#"SELECT
                connection_id,
                app_id,
                owner_user_id,
                provider_account_id,
                external_user_id,
                status,
                connection_json,
                created_at,
                updated_at
              FROM app_connections
              WHERE app_id = ?1 AND connection_id = ?2
              LIMIT 1"#,
        )
        .bind(app_id.to_string())
        .bind(connection_id.to_string())
        .fetch_optional(self.conn.pool())
        .await?;

        row.map(map_app_connection_row).transpose()
    }

    pub async fn find_app_connection_by_oauth_state(
        &self,
        app_id: &Uri,
        oauth_state: &str,
    ) -> Result<Option<AppConnectionRecord>> {
        let row = sqlx::query(
            r#"SELECT
                connection_id,
                app_id,
                owner_user_id,
                provider_account_id,
                external_user_id,
                status,
                connection_json,
                created_at,
                updated_at
              FROM app_connections
              WHERE app_id = ?1
                AND json_extract(connection_json, '$.oauth_state') = ?2
              LIMIT 1"#,
        )
        .bind(app_id.to_string())
        .bind(oauth_state)
        .fetch_optional(self.conn.pool())
        .await?;

        row.map(map_app_connection_row).transpose()
    }

    pub async fn upsert_app_connection(
        &self,
        app_id: &Uri,
        connection_id: &Uri,
        owner_user_id: Option<&Uri>,
        provider_account_id: Option<&str>,
        external_user_id: Option<&str>,
        status: &str,
        connection_json: &serde_json::Value,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO app_connections(
              connection_id, app_id, owner_user_id, provider_account_id, external_user_id, status, connection_json, created_at, updated_at
            )
            VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(connection_id) DO UPDATE SET
              app_id = excluded.app_id,
              owner_user_id = excluded.owner_user_id,
              provider_account_id = excluded.provider_account_id,
              external_user_id = excluded.external_user_id,
              status = excluded.status,
              connection_json = excluded.connection_json,
              updated_at = excluded.updated_at
            "#,
        )
        .bind(connection_id.to_string())
        .bind(app_id.to_string())
        .bind(owner_user_id.map(ToString::to_string))
        .bind(provider_account_id.map(str::to_string))
        .bind(external_user_id.map(str::to_string))
        .bind(status.to_string())
        .bind(connection_json.to_string())
        .bind(now.clone())
        .bind(now)
        .execute(self.conn.pool())
        .await?;
        Ok(())
    }

    pub async fn delete_app_connection(&self, app_id: &Uri, connection_id: &Uri) -> Result<u64> {
        let deleted =
            sqlx::query("DELETE FROM app_connections WHERE app_id = ?1 AND connection_id = ?2")
                .bind(app_id.to_string())
                .bind(connection_id.to_string())
                .execute(self.conn.pool())
                .await?
                .rows_affected();
        Ok(deleted)
    }

    pub async fn list_app_secrets(
        &self,
        app_id: &Uri,
        connection_id: Option<&Uri>,
        limit: usize,
    ) -> Result<Vec<AppSecretRecord>> {
        let limit = i64::try_from(limit).unwrap_or(100);
        let rows = if let Some(connection_id) = connection_id {
            sqlx::query(
                r#"SELECT
                    secret_id,
                    app_id,
                    connection_id,
                    key,
                    value,
                    kind,
                    created_at,
                    updated_at
                  FROM app_secrets
                  WHERE app_id = ?1 AND connection_id = ?2
                  ORDER BY updated_at DESC, key ASC
                  LIMIT ?3"#,
            )
            .bind(app_id.to_string())
            .bind(connection_id.to_string())
            .bind(limit)
            .fetch_all(self.conn.pool())
            .await?
        } else {
            sqlx::query(
                r#"SELECT
                    secret_id,
                    app_id,
                    connection_id,
                    key,
                    value,
                    kind,
                    created_at,
                    updated_at
                  FROM app_secrets
                  WHERE app_id = ?1
                  ORDER BY updated_at DESC, key ASC
                  LIMIT ?2"#,
            )
            .bind(app_id.to_string())
            .bind(limit)
            .fetch_all(self.conn.pool())
            .await?
        };

        rows.into_iter().map(map_app_secret_row).collect()
    }

    pub async fn get_app_secret(
        &self,
        app_id: &Uri,
        secret_id: &Uri,
    ) -> Result<Option<AppSecretRecord>> {
        let row = sqlx::query(
            r#"SELECT
                secret_id,
                app_id,
                connection_id,
                key,
                value,
                kind,
                created_at,
                updated_at
              FROM app_secrets
              WHERE app_id = ?1 AND secret_id = ?2
              LIMIT 1"#,
        )
        .bind(app_id.to_string())
        .bind(secret_id.to_string())
        .fetch_optional(self.conn.pool())
        .await?;
        row.map(map_app_secret_row).transpose()
    }

    pub async fn upsert_app_secret(
        &self,
        app_id: &Uri,
        secret_id: &Uri,
        connection_id: Option<&Uri>,
        key: &str,
        value: &str,
        kind: &str,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO app_secrets(
              secret_id, app_id, connection_id, key, value, kind, created_at, updated_at
            )
            VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(secret_id) DO UPDATE SET
              app_id = excluded.app_id,
              connection_id = excluded.connection_id,
              key = excluded.key,
              value = excluded.value,
              kind = excluded.kind,
              updated_at = excluded.updated_at
            "#,
        )
        .bind(secret_id.to_string())
        .bind(app_id.to_string())
        .bind(connection_id.map(ToString::to_string))
        .bind(key.to_string())
        .bind(value.to_string())
        .bind(kind.to_string())
        .bind(now.clone())
        .bind(now)
        .execute(self.conn.pool())
        .await?;
        Ok(())
    }

    pub async fn delete_app_secret(&self, app_id: &Uri, secret_id: &Uri) -> Result<u64> {
        let deleted = sqlx::query("DELETE FROM app_secrets WHERE app_id = ?1 AND secret_id = ?2")
            .bind(app_id.to_string())
            .bind(secret_id.to_string())
            .execute(self.conn.pool())
            .await?
            .rows_affected();
        Ok(deleted)
    }

    pub async fn list_app_capabilities(
        &self,
        app_id: &Uri,
        limit: usize,
    ) -> Result<Vec<AppCapabilityRecord>> {
        let limit = i64::try_from(limit).unwrap_or(100);
        let app_id = app_id.to_string();
        let rows = sqlx::query!(
            r#"SELECT
                capability_id as "capability_id!: String",
                app_id as "app_id!: String",
                name as "name!: String",
                hint as "hint!: String",
                mode as "mode!: String",
                instructions as "instructions!: String",
                status as "status!: String",
                created_at as "created_at!: String",
                updated_at as "updated_at!: String"
            FROM app_capabilities
            WHERE app_id = ?1
            ORDER BY updated_at DESC, name ASC
            LIMIT ?2"#,
            app_id,
            limit,
        )
        .fetch_all(self.conn.pool())
        .await?;

        rows.into_iter()
            .map(|row| {
                Ok(AppCapabilityRecord {
                    capability_id: Uri::parse(&row.capability_id)?,
                    app_id: Uri::parse(&row.app_id)?,
                    name: row.name,
                    hint: row.hint,
                    mode: row.mode,
                    instructions: row.instructions,
                    status: row.status,
                    created_at: parse_ts(&row.created_at)?,
                    updated_at: parse_ts(&row.updated_at)?,
                })
            })
            .collect()
    }

    pub async fn get_app_capability(
        &self,
        app_id: &Uri,
        capability_id: &Uri,
    ) -> Result<Option<AppCapabilityRecord>> {
        let app_id = app_id.to_string();
        let capability_id = capability_id.to_string();
        let row = sqlx::query!(
            r#"SELECT
                capability_id as "capability_id!: String",
                app_id as "app_id!: String",
                name as "name!: String",
                hint as "hint!: String",
                mode as "mode!: String",
                instructions as "instructions!: String",
                status as "status!: String",
                created_at as "created_at!: String",
                updated_at as "updated_at!: String"
            FROM app_capabilities
            WHERE app_id = ?1 AND capability_id = ?2
            LIMIT 1"#,
            app_id,
            capability_id,
        )
        .fetch_optional(self.conn.pool())
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };

        Ok(Some(AppCapabilityRecord {
            capability_id: Uri::parse(&row.capability_id)?,
            app_id: Uri::parse(&row.app_id)?,
            name: row.name,
            hint: row.hint,
            mode: row.mode,
            instructions: row.instructions,
            status: row.status,
            created_at: parse_ts(&row.created_at)?,
            updated_at: parse_ts(&row.updated_at)?,
        }))
    }

    pub async fn upsert_app_capability(
        &self,
        app_id: &Uri,
        capability_id: &Uri,
        name: &str,
        hint: &str,
        mode: &str,
        instructions: &str,
        status: &str,
    ) -> Result<()> {
        let capability_id = capability_id.to_string();
        let app_id = app_id.to_string();
        let name = name.to_string();
        let hint = hint.to_string();
        let mode = mode.to_string();
        let instructions = instructions.to_string();
        let status = status.to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query!(
            r#"
            INSERT INTO app_capabilities(
              capability_id, app_id, name, hint, mode, instructions, status, updated_at
            )
            VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(capability_id) DO UPDATE SET
              app_id = excluded.app_id,
              name = excluded.name,
              hint = excluded.hint,
              mode = excluded.mode,
              instructions = excluded.instructions,
              status = excluded.status,
              updated_at = excluded.updated_at
            "#,
            capability_id,
            app_id,
            name,
            hint,
            mode,
            instructions,
            status,
            now,
        )
        .execute(self.conn.pool())
        .await?;
        Ok(())
    }

    pub async fn delete_app_capability(&self, app_id: &Uri, capability_id: &Uri) -> Result<u64> {
        let app_id = app_id.to_string();
        let capability_id = capability_id.to_string();
        let deleted = sqlx::query!(
            "DELETE FROM app_capabilities WHERE app_id = ?1 AND capability_id = ?2",
            app_id,
            capability_id,
        )
        .execute(self.conn.pool())
        .await?
        .rows_affected();
        Ok(deleted)
    }
}

fn map_app_connection_row(row: sqlx::sqlite::SqliteRow) -> Result<AppConnectionRecord> {
    let connection_json = row
        .try_get::<String, _>("connection_json")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(|value| serde_json::from_str(&value))
        .transpose()?
        .unwrap_or_else(|| json!({}));
    Ok(AppConnectionRecord {
        connection_id: Uri::parse(&row.try_get::<String, _>("connection_id")?)?,
        app_id: Uri::parse(&row.try_get::<String, _>("app_id")?)?,
        owner_user_id: row
            .try_get::<Option<String>, _>("owner_user_id")?
            .map(|value| Uri::parse(&value))
            .transpose()?,
        provider_account_id: row.try_get::<Option<String>, _>("provider_account_id")?,
        external_user_id: row.try_get::<Option<String>, _>("external_user_id")?,
        status: row.try_get::<String, _>("status")?,
        connection_json,
        created_at: parse_ts(&row.try_get::<String, _>("created_at")?)?,
        updated_at: parse_ts(&row.try_get::<String, _>("updated_at")?)?,
    })
}

fn map_app_secret_row(row: sqlx::sqlite::SqliteRow) -> Result<AppSecretRecord> {
    Ok(AppSecretRecord {
        secret_id: Uri::parse(&row.try_get::<String, _>("secret_id")?)?,
        app_id: Uri::parse(&row.try_get::<String, _>("app_id")?)?,
        connection_id: row
            .try_get::<Option<String>, _>("connection_id")?
            .map(|value| Uri::parse(&value))
            .transpose()?,
        key: row.try_get::<String, _>("key")?,
        value: row.try_get::<String, _>("value")?,
        kind: row.try_get::<String, _>("kind")?,
        created_at: parse_ts(&row.try_get::<String, _>("created_at")?)?,
        updated_at: parse_ts(&row.try_get::<String, _>("updated_at")?)?,
    })
}
