mod sqlite_store;

pub(crate) use sqlite_store::SqliteFactStore;
pub use sqlite_store::{FactArity, FactInput, FactRecord, FactValue, StateFactsResult, Uri};
