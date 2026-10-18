//! One append-only schema history for every connection to the private database.

use arrpi_core::error::ApiError;
use rusqlite::Connection;
use rusqlite_migration::{Error, M, MigrationDefinitionError, Migrations};

const STEPS: &[M<'static>] = &[
    M::up(include_str!("migrations/001_initial.sql")),
    M::up(include_str!("migrations/002_instance.sql")),
];
const MIGRATIONS: Migrations<'static> = Migrations::from_slice(STEPS);

pub(super) fn apply(connection: &mut Connection) -> Result<(), ApiError> {
    connection
        .busy_timeout(std::time::Duration::from_millis(500))
        .map_err(|error| super::settings::sql_error(&error))?;
    MIGRATIONS.to_latest(connection).map_err(|error| {
        let (category, code) = match &error {
            Error::RusqliteError { err, .. } => ("sqlite", err.sqlite_error_code()),

            Error::InvalidUserVersion => ("invalid_schema_version", None),

            Error::MigrationDefinition(MigrationDefinitionError::DatabaseTooFarAhead) => {
                ("database_newer_than_binary", None)
            }

            Error::MigrationDefinition(_) => ("migration_definition", None),

            Error::SpecifiedSchemaVersion(_) => ("migration_target", None),

            _ => ("migration", None),
        };

        tracing::error!(event = "database_migration", category, code = ?code);
        ApiError::INTERNAL
    })
}

#[cfg(test)]
mod tests;
