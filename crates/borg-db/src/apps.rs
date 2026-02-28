use anyhow::Result;
use borg_core::Uri;
use chrono::Utc;

use crate::utils::parse_ts;
use crate::{AppCapabilityRecord, AppRecord, BorgDb};

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
        self.upsert_app_with_built_in(app_id, name, slug, description, status, false)
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
        self.upsert_app_with_built_in(app_id, name, slug, description, status, true)
            .await
    }

    async fn upsert_app_with_built_in(
        &self,
        app_id: &Uri,
        name: &str,
        slug: &str,
        description: &str,
        status: &str,
        built_in: bool,
    ) -> Result<()> {
        let app_id = app_id.to_string();
        let name = name.to_string();
        let slug = slug.to_string();
        let description = description.to_string();
        let status = status.to_string();
        let now = Utc::now().to_rfc3339();
        let created_at = now.clone();
        let updated_at = now;
        let built_in_i64 = if built_in { 1_i64 } else { 0_i64 };
        sqlx::query!(
            r#"
            INSERT INTO apps(app_id, name, slug, description, status, built_in, created_at, updated_at)
            VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(app_id) DO UPDATE SET
              name = excluded.name,
              slug = excluded.slug,
              description = excluded.description,
              status = excluded.status,
              updated_at = excluded.updated_at
            "#,
            app_id,
            name,
            slug,
            description,
            status,
            built_in_i64,
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
