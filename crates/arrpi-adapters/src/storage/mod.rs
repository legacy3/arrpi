//! Private `SQLite` settings, observation counters and revocable feed capabilities.

mod audit;
mod events;
mod feeds;
mod migrations;
mod settings;
mod stats;

use rusqlite::Connection;

pub use audit::AuditFilter;
pub use events::SqliteEvents;
pub use feeds::{FeedAction, FeedLinks, SubscriptionStore};
pub use settings::{Store, public, restore_secrets};

/// An owned `SQLite` connection; paths and opening policy belong to the application.
#[derive(Debug)]
pub struct Database(Connection);

impl From<Connection> for Database {
    fn from(connection: Connection) -> Self {
        Self(connection)
    }
}

fn unsigned(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u64> {
    let value: i64 = row.get(index)?;

    u64::try_from(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })
}

fn optional_unsigned(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Option<u64>> {
    let value: Option<i64> = row.get(index)?;

    value
        .map(|number| {
            u64::try_from(number).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    index,
                    rusqlite::types::Type::Integer,
                    Box::new(error),
                )
            })
        })
        .transpose()
}
