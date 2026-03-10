use anyhow::{Context, Result, anyhow};
use borg_core::Uri;
use chrono::Utc;
use sqlx::Row;

use crate::utils::parse_ts;
use crate::{BorgDb, DevModeProjectRecord, DevModeSpecRecord};

impl BorgDb {
    pub async fn upsert_devmode_project(
        &self,
        project_id: &Uri,
        name: &str,
        root_path: &str,
        description: &str,
        status: &str,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let normalized_status = normalize_project_status(status)?;
        let normalized_name = name.trim();
        if normalized_name.is_empty() {
            return Err(anyhow!("project name is required"));
        }
        sqlx::query(
            r#"
            INSERT INTO devmode_projects(project_id, name, root_path, description, status, created_at, updated_at)
            VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(project_id) DO UPDATE SET
              name = excluded.name,
              root_path = excluded.root_path,
              description = excluded.description,
              status = excluded.status,
              updated_at = excluded.updated_at
            "#,
        )
        .bind(project_id.to_string())
        .bind(normalized_name)
        .bind(root_path.trim())
        .bind(description.trim())
        .bind(normalized_status)
        .bind(now.clone())
        .bind(now)
        .execute(self.pool())
        .await
        .context("failed to upsert devmode project")?;
        Ok(())
    }

    pub async fn list_devmode_projects(&self, limit: usize) -> Result<Vec<DevModeProjectRecord>> {
        let limit = i64::try_from(limit).unwrap_or(100);
        let rows = sqlx::query(
            r#"
            SELECT project_id, name, root_path, description, status, created_at, updated_at
            FROM devmode_projects
            ORDER BY updated_at DESC
            LIMIT ?1
            "#,
        )
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .context("failed to list devmode projects")?;

        rows.into_iter().map(devmode_project_from_row).collect()
    }

    pub async fn get_devmode_project(
        &self,
        project_id: &Uri,
    ) -> Result<Option<DevModeProjectRecord>> {
        let row = sqlx::query(
            r#"
            SELECT project_id, name, root_path, description, status, created_at, updated_at
            FROM devmode_projects
            WHERE project_id = ?1
            LIMIT 1
            "#,
        )
        .bind(project_id.to_string())
        .fetch_optional(self.pool())
        .await
        .context("failed to get devmode project")?;

        row.map(devmode_project_from_row).transpose()
    }

    pub async fn upsert_devmode_spec(
        &self,
        spec_id: &Uri,
        project_id: &Uri,
        title: &str,
        body: &str,
        status: &str,
    ) -> Result<()> {
        let exists = self.get_devmode_project(project_id).await?;
        if exists.is_none() {
            return Err(anyhow!("devmode project not found"));
        }

        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO devmode_specs(
                spec_id, project_id, title, body, status, root_task_uri, created_at, updated_at
            )
            VALUES(?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7)
            ON CONFLICT(spec_id) DO UPDATE SET
              project_id = excluded.project_id,
              title = excluded.title,
              body = excluded.body,
              status = excluded.status,
              updated_at = excluded.updated_at
            "#,
        )
        .bind(spec_id.to_string())
        .bind(project_id.to_string())
        .bind(title.trim())
        .bind(body.trim())
        .bind(status.trim())
        .bind(now.clone())
        .bind(now)
        .execute(self.pool())
        .await
        .context("failed to upsert devmode spec")?;
        Ok(())
    }

    pub async fn list_devmode_specs(
        &self,
        project_id: Option<&Uri>,
        limit: usize,
    ) -> Result<Vec<DevModeSpecRecord>> {
        let limit = i64::try_from(limit).unwrap_or(100);
        let rows = if let Some(project_id) = project_id {
            sqlx::query(
                r#"
                SELECT spec_id, project_id, title, body, status, root_task_uri, created_at, updated_at
                FROM devmode_specs
                WHERE project_id = ?1
                ORDER BY updated_at DESC
                LIMIT ?2
                "#,
            )
            .bind(project_id.to_string())
            .bind(limit)
            .fetch_all(self.pool())
            .await
            .context("failed to list devmode specs by project")?
        } else {
            sqlx::query(
                r#"
                SELECT spec_id, project_id, title, body, status, root_task_uri, created_at, updated_at
                FROM devmode_specs
                ORDER BY updated_at DESC
                LIMIT ?1
                "#,
            )
            .bind(limit)
            .fetch_all(self.pool())
            .await
            .context("failed to list devmode specs")?
        };

        rows.into_iter().map(devmode_spec_from_row).collect()
    }

    pub async fn get_devmode_spec(&self, spec_id: &Uri) -> Result<Option<DevModeSpecRecord>> {
        let row = sqlx::query(
            r#"
            SELECT spec_id, project_id, title, body, status, root_task_uri, created_at, updated_at
            FROM devmode_specs
            WHERE spec_id = ?1
            LIMIT 1
            "#,
        )
        .bind(spec_id.to_string())
        .fetch_optional(self.pool())
        .await
        .context("failed to get devmode spec")?;

        row.map(devmode_spec_from_row).transpose()
    }

    pub async fn mark_devmode_spec_taskgraphed(
        &self,
        spec_id: &Uri,
        root_task_uri: &str,
    ) -> Result<u64> {
        let now = Utc::now().to_rfc3339();
        let updated = sqlx::query(
            r#"
            UPDATE devmode_specs
            SET root_task_uri = ?2,
                status = 'TASK_GRAPHED',
                updated_at = ?3
            WHERE spec_id = ?1
            "#,
        )
        .bind(spec_id.to_string())
        .bind(root_task_uri.trim())
        .bind(now)
        .execute(self.pool())
        .await
        .context("failed to mark devmode spec taskgraphed")?
        .rows_affected();
        Ok(updated)
    }
}

fn devmode_project_from_row(row: sqlx::sqlite::SqliteRow) -> Result<DevModeProjectRecord> {
    Ok(DevModeProjectRecord {
        project_id: Uri::parse(&row.try_get::<String, _>("project_id")?)?,
        name: row.try_get("name")?,
        root_path: row.try_get("root_path")?,
        description: row.try_get("description")?,
        status: row.try_get("status")?,
        created_at: parse_ts(&row.try_get::<String, _>("created_at")?)?,
        updated_at: parse_ts(&row.try_get::<String, _>("updated_at")?)?,
    })
}

fn normalize_project_status(status: &str) -> Result<&'static str> {
    match status.trim().to_ascii_uppercase().as_str() {
        "ONGOING" => Ok("ONGOING"),
        "ARCHIVED" => Ok("ARCHIVED"),
        _ => Err(anyhow!("invalid project status")),
    }
}

fn devmode_spec_from_row(row: sqlx::sqlite::SqliteRow) -> Result<DevModeSpecRecord> {
    let root_task_uri = row
        .try_get::<Option<String>, _>("root_task_uri")?
        .map(|value| Uri::parse(&value))
        .transpose()?;
    Ok(DevModeSpecRecord {
        spec_id: Uri::parse(&row.try_get::<String, _>("spec_id")?)?,
        project_id: Uri::parse(&row.try_get::<String, _>("project_id")?)?,
        title: row.try_get("title")?,
        body: row.try_get("body")?,
        status: row.try_get("status")?,
        root_task_uri,
        created_at: parse_ts(&row.try_get::<String, _>("created_at")?)?,
        updated_at: parse_ts(&row.try_get::<String, _>("updated_at")?)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp_db_path(test_name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "borg-db-devmode-{test_name}-{}.db",
            uuid::Uuid::new_v4()
        ));
        path
    }

    #[tokio::test]
    async fn devmode_project_spec_roundtrip() -> Result<()> {
        let path = tmp_db_path("roundtrip");
        let db = BorgDb::open_local(
            path.to_str()
                .ok_or_else(|| anyhow::anyhow!("invalid temp db path"))?,
        )
        .await?;
        db.migrate().await?;

        let project_id = Uri::from_parts("devmode", "project", Some("p1"))?;
        let spec_id = Uri::from_parts("devmode", "spec", Some("s1"))?;
        db.upsert_devmode_project(
            &project_id,
            "Test Project",
            "/tmp/repo",
            "test project",
            "ONGOING",
        )
        .await?;
        db.upsert_devmode_spec(
            &spec_id,
            &project_id,
            "Build feature X",
            "Implement feature X end-to-end.",
            "DRAFT",
        )
        .await?;

        let fetched = db.get_devmode_spec(&spec_id).await?.expect("spec exists");
        assert_eq!(fetched.title, "Build feature X");
        assert_eq!(fetched.status, "DRAFT");
        assert!(fetched.root_task_uri.is_none());

        let fetched_project = db
            .get_devmode_project(&project_id)
            .await?
            .expect("project exists");
        assert_eq!(fetched_project.name, "Test Project");
        assert_eq!(fetched_project.description, "test project");
        assert_eq!(fetched_project.status, "ONGOING");

        let updated = db
            .mark_devmode_spec_taskgraphed(&spec_id, "borg:task:test")
            .await?;
        assert_eq!(updated, 1);
        let fetched = db.get_devmode_spec(&spec_id).await?.expect("spec exists");
        assert_eq!(fetched.status, "TASK_GRAPHED");
        assert_eq!(
            fetched.root_task_uri.as_ref().map(Uri::as_str),
            Some("borg:task:test")
        );
        Ok(())
    }
}
