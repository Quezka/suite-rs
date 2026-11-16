//! Storage and the outside world (port of `quire/infrastructure`). The database file is the Python app's, table for
//! table and trigger for trigger, so either app can open it.

pub mod classeviva;
pub mod clock;
pub mod cloud;
pub mod credentials;
pub mod firebase;
pub mod http;
pub mod login_item;
pub mod paths;
pub mod quiet;
pub mod quire_server;
pub mod repositories;
pub mod sqlite;
pub mod system_calendar;
pub mod updates;
pub mod sync_store;
