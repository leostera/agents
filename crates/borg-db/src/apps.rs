use anyhow::Result;
use borg_core::Uri;
use chrono::Utc;

use crate::utils::parse_ts;
use crate::{AppCapabilityRecord, AppRecord, BorgDb};

impl BorgDb {
    pub async fn list_apps(&self, limit: usize) -> Result<Vec<AppRecord>> {
        let limit = i64::try_from(limit).unwrap_or(100);
        let mut rows = self
            .conn
            .query(
                "SELECT app_id, name, slug, description, status, created_at, updated_at
                 FROM apps
                 ORDER BY updated_at DESC, slug ASC
                 LIMIT ?1",
                (limit,),
            )
            .await?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            let created_at_raw: String = row.get(5)?;
            let updated_at_raw: String = row.get(6)?;
            out.push(AppRecord {
                app_id: Uri::parse(&row.get::<String>(0)?)?,
                name: row.get(1)?,
                slug: row.get(2)?,
                description: row.get(3)?,
                status: row.get(4)?,
                created_at: parse_ts(&created_at_raw)?,
                updated_at: parse_ts(&updated_at_raw)?,
            });
        }
        Ok(out)
    }

    pub async fn get_app(&self, app_id: &Uri) -> Result<Option<AppRecord>> {
        let mut rows = self
            .conn
            .query(
                "SELECT app_id, name, slug, description, status, created_at, updated_at
                 FROM apps
                 WHERE app_id = ?1
                 LIMIT 1",
                (app_id.to_string(),),
            )
            .await?;

        let Some(row) = rows.next().await? else {
            return Ok(None);
        };

        let created_at_raw: String = row.get(5)?;
        let updated_at_raw: String = row.get(6)?;
        Ok(Some(AppRecord {
            app_id: Uri::parse(&row.get::<String>(0)?)?,
            name: row.get(1)?,
            slug: row.get(2)?,
            description: row.get(3)?,
            status: row.get(4)?,
            created_at: parse_ts(&created_at_raw)?,
            updated_at: parse_ts(&updated_at_raw)?,
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
        let now = Utc::now().to_rfc3339();
        self.conn
            .execute(
                r#"
                INSERT INTO apps(app_id, name, slug, description, status, created_at, updated_at)
                VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)
                ON CONFLICT(app_id) DO UPDATE SET
                  name = excluded.name,
                  slug = excluded.slug,
                  description = excluded.description,
                  status = excluded.status,
                  updated_at = excluded.updated_at
                "#,
                (
                    app_id.to_string(),
                    name.to_string(),
                    slug.to_string(),
                    description.to_string(),
                    status.to_string(),
                    now.clone(),
                    now,
                ),
            )
            .await?;
        Ok(())
    }

    pub async fn delete_app(&self, app_id: &Uri) -> Result<u64> {
        let deleted = self
            .conn
            .execute("DELETE FROM apps WHERE app_id = ?1", (app_id.to_string(),))
            .await?;
        Ok(deleted)
    }

    pub async fn list_app_capabilities(
        &self,
        app_id: &Uri,
        limit: usize,
    ) -> Result<Vec<AppCapabilityRecord>> {
        let limit = i64::try_from(limit).unwrap_or(100);
        let mut rows = self
            .conn
            .query(
                "SELECT capability_id, app_id, name, hint, mode, instructions, status, created_at, updated_at
                 FROM app_capabilities
                 WHERE app_id = ?1
                 ORDER BY updated_at DESC, name ASC
                 LIMIT ?2",
                (app_id.to_string(), limit),
            )
            .await?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            let created_at_raw: String = row.get(7)?;
            let updated_at_raw: String = row.get(8)?;
            out.push(AppCapabilityRecord {
                capability_id: Uri::parse(&row.get::<String>(0)?)?,
                app_id: Uri::parse(&row.get::<String>(1)?)?,
                name: row.get(2)?,
                hint: row.get(3)?,
                mode: row.get(4)?,
                instructions: row.get(5)?,
                status: row.get(6)?,
                created_at: parse_ts(&created_at_raw)?,
                updated_at: parse_ts(&updated_at_raw)?,
            });
        }
        Ok(out)
    }

    pub async fn get_app_capability(
        &self,
        app_id: &Uri,
        capability_id: &Uri,
    ) -> Result<Option<AppCapabilityRecord>> {
        let mut rows = self
            .conn
            .query(
                "SELECT capability_id, app_id, name, hint, mode, instructions, status, created_at, updated_at
                 FROM app_capabilities
                 WHERE app_id = ?1 AND capability_id = ?2
                 LIMIT 1",
                (app_id.to_string(), capability_id.to_string()),
            )
            .await?;

        let Some(row) = rows.next().await? else {
            return Ok(None);
        };

        let created_at_raw: String = row.get(7)?;
        let updated_at_raw: String = row.get(8)?;
        Ok(Some(AppCapabilityRecord {
            capability_id: Uri::parse(&row.get::<String>(0)?)?,
            app_id: Uri::parse(&row.get::<String>(1)?)?,
            name: row.get(2)?,
            hint: row.get(3)?,
            mode: row.get(4)?,
            instructions: row.get(5)?,
            status: row.get(6)?,
            created_at: parse_ts(&created_at_raw)?,
            updated_at: parse_ts(&updated_at_raw)?,
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
        let now = Utc::now().to_rfc3339();
        self.conn
            .execute(
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
                (
                    capability_id.to_string(),
                    app_id.to_string(),
                    name.to_string(),
                    hint.to_string(),
                    mode.to_string(),
                    instructions.to_string(),
                    status.to_string(),
                    now,
                ),
            )
            .await?;
        Ok(())
    }

    pub async fn delete_app_capability(&self, app_id: &Uri, capability_id: &Uri) -> Result<u64> {
        let deleted = self
            .conn
            .execute(
                "DELETE FROM app_capabilities WHERE app_id = ?1 AND capability_id = ?2",
                (app_id.to_string(), capability_id.to_string()),
            )
            .await?;
        Ok(deleted)
    }
}
