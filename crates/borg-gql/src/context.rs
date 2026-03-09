use async_graphql::{Error, ErrorExtensions, Result as GqlResult};
use borg_db::BorgDb;
use borg_exec::BorgActorManager;
use borg_memory::MemoryStore;
use std::sync::Arc;

pub(crate) const DEFAULT_PAGE_SIZE: usize = 25;
pub(crate) const MAX_PAGE_SIZE: usize = 200;
pub(crate) const DEFAULT_SUBSCRIPTION_POLL_MS: u64 = 500;
pub(crate) const MIN_SUBSCRIPTION_POLL_MS: u64 = 100;
pub(crate) const MAX_SUBSCRIPTION_POLL_MS: u64 = 5_000;

/// Runtime context for GraphQL resolvers.
#[derive(Clone)]
pub struct BorgGqlData {
    pub(crate) db: BorgDb,
    pub(crate) memory: MemoryStore,
    pub(crate) supervisor: Arc<BorgActorManager>,
    default_page_size: usize,
    max_page_size: usize,
}

impl BorgGqlData {
    /// Creates a new GraphQL resolver context.
    pub fn new(db: BorgDb, memory: MemoryStore, supervisor: Arc<BorgActorManager>) -> Self {
        Self {
            db,
            memory,
            supervisor,
            default_page_size: DEFAULT_PAGE_SIZE,
            max_page_size: MAX_PAGE_SIZE,
        }
    }

    pub(crate) fn normalize_first(&self, first: Option<i32>) -> GqlResult<usize> {
        let raw = first.unwrap_or(self.default_page_size as i32);
        if raw <= 0 {
            return Err(
                Error::new("first must be greater than zero").extend_with(|_, e| {
                    e.set("code", "BAD_REQUEST");
                }),
            );
        }
        Ok((raw as usize).min(self.max_page_size))
    }
}
