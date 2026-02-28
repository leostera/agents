mod agents;
mod core;
mod llm_calls;
mod migrations;
mod policies;
mod ports;
mod providers;
mod sessions;
mod tasks;
mod users;
mod utils;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::query::Query;
use sqlx::sqlite::{SqliteArguments, SqliteRow};
use sqlx::{Decode, Encode, Row, Sqlite, SqlitePool, Type};

use borg_core::{TaskKind, Uri};

#[derive(Clone)]
pub struct BorgDb {
    conn: CompatConn,
}

#[derive(Clone)]
pub(crate) struct CompatConn {
    pool: SqlitePool,
}

impl CompatConn {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn execute<'q, A>(&self, sql: &'q str, args: A) -> anyhow::Result<u64>
    where
        A: SqlBind<'q>,
    {
        let query = args.bind(sqlx::query(sql));
        let result = query.execute(&self.pool).await?;
        Ok(result.rows_affected())
    }

    pub async fn query<'q, A>(&self, sql: &'q str, args: A) -> anyhow::Result<CompatRows>
    where
        A: SqlBind<'q>,
    {
        let query = args.bind(sqlx::query(sql));
        let rows = query.fetch_all(&self.pool).await?;
        Ok(CompatRows {
            iter: rows.into_iter(),
        })
    }
}

pub(crate) struct CompatRows {
    iter: std::vec::IntoIter<SqliteRow>,
}

impl CompatRows {
    pub async fn next(&mut self) -> anyhow::Result<Option<CompatRow>> {
        Ok(self.iter.next().map(|row| CompatRow { row }))
    }
}

pub(crate) struct CompatRow {
    row: SqliteRow,
}

pub(crate) trait SqlBind<'q> {
    fn bind(
        self,
        query: Query<'q, Sqlite, SqliteArguments<'q>>,
    ) -> Query<'q, Sqlite, SqliteArguments<'q>>;
}

impl<'q> SqlBind<'q> for () {
    fn bind(
        self,
        query: Query<'q, Sqlite, SqliteArguments<'q>>,
    ) -> Query<'q, Sqlite, SqliteArguments<'q>> {
        query
    }
}

macro_rules! impl_sql_bind_tuple {
    ($($name:ident),+) => {
        impl<'q, $($name),+> SqlBind<'q> for ($($name,)+)
        where
            $(
                $name: 'q + Send + Encode<'q, Sqlite> + Type<Sqlite>,
            )+
        {
            #[allow(non_snake_case)]
            fn bind(self, query: Query<'q, Sqlite, SqliteArguments<'q>>) -> Query<'q, Sqlite, SqliteArguments<'q>> {
                let ($($name,)+) = self;
                query$(.bind($name))+
            }
        }
    };
}

impl_sql_bind_tuple!(A);
impl_sql_bind_tuple!(A, B);
impl_sql_bind_tuple!(A, B, C);
impl_sql_bind_tuple!(A, B, C, D);
impl_sql_bind_tuple!(A, B, C, D, E);
impl_sql_bind_tuple!(A, B, C, D, E, F);
impl_sql_bind_tuple!(A, B, C, D, E, F, G);
impl_sql_bind_tuple!(A, B, C, D, E, F, G, H);

impl CompatRow {
    pub fn get<T>(&self, index: usize) -> anyhow::Result<T>
    where
        T: Type<Sqlite> + for<'r> Decode<'r, Sqlite>,
    {
        Ok(self.row.try_get(index)?)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NewTask {
    pub kind: TaskKind,
    pub payload: Value,
    pub parent_task_id: Option<Uri>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentSpecRecord {
    pub agent_id: Uri,
    pub name: String,
    pub model: String,
    pub system_prompt: String,
    pub tools: Value,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SessionRecord {
    pub session_id: Uri,
    pub users: Vec<Uri>,
    pub port: Uri,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SessionMessageRecord {
    pub message_id: Uri,
    pub session_id: Uri,
    pub message_index: i64,
    pub payload: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserRecord {
    pub user_key: Uri,
    pub profile: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderRecord {
    pub provider: String,
    pub api_key: String,
    pub enabled: bool,
    pub tokens_used: u64,
    pub last_used: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PortRecord {
    pub port_id: Uri,
    pub provider: String,
    pub port_name: String,
    pub enabled: bool,
    pub allows_guests: bool,
    pub default_agent_id: Option<Uri>,
    pub settings: Value,
    pub active_sessions: u64,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PolicyRecord {
    pub policy_id: Uri,
    pub policy: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PolicyUseRecord {
    pub policy_id: Uri,
    pub entity_id: Uri,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LlmCallRecord {
    pub call_id: String,
    pub provider: String,
    pub capability: String,
    pub model: String,
    pub success: bool,
    pub status_code: Option<u16>,
    pub status_reason: Option<String>,
    pub http_reason: Option<String>,
    pub error: Option<String>,
    pub latency_ms: Option<u64>,
    pub sent_at: DateTime<Utc>,
    pub received_at: Option<DateTime<Utc>>,
    pub request_json: Value,
    pub response_json: Value,
    pub response_body: String,
}
