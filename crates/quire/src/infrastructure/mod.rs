//! Storage and the outside world (port of `quire/infrastructure`). The database file is the Python app's, table for
//! table and trigger for trigger, so either app can open it.

pub mod sqlite;
pub mod sync_store;
