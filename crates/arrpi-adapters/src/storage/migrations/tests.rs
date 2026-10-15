//! Fresh initialization, reopen behavior and failure atomicity.

use super::{MIGRATIONS, apply};
use rusqlite::Connection;

fn version(connection: &Connection) -> i64 {
    connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("schema version")
}

fn schema(connection: &Connection) -> Vec<(String, String)> {
    connection
        .prepare("SELECT name, sql FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%' ORDER BY name")
        .expect("schema statement")
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("schema rows")
        .collect::<Result<_, _>>()
        .expect("schema values")
}

#[test]
fn migration_history_is_valid_and_database_reopens_unchanged() {
    MIGRATIONS.validate().expect("valid migration history");
    let mut connection = Connection::open_in_memory().expect("SQLite");

    apply(&mut connection).expect("fresh migration");
    let initial = schema(&connection);

    assert_eq!(version(&connection), 1);
    assert_eq!(initial.len(), 6, "five tables and the audit index");
    apply(&mut connection).expect("repeat migration");
    assert_eq!(schema(&connection), initial);
    assert_eq!(version(&connection), 1);
}

#[test]
fn failed_migration_rolls_back_all_schema_changes_and_its_version() {
    let mut connection = Connection::open_in_memory().expect("SQLite");

    connection.execute_batch("CREATE TABLE write_audit (id INTEGER PRIMARY KEY, note TEXT); INSERT INTO write_audit VALUES(1,'private row');").expect("conflicting schema");
    let before = schema(&connection);

    apply(&mut connection).expect_err("conflicting audit table");
    assert!(
        connection.is_autocommit(),
        "failed migration closes its transaction"
    );
    assert_eq!(version(&connection), 0);
    assert_eq!(schema(&connection), before);
    assert_eq!(
        connection
            .query_row("SELECT note FROM write_audit", [], |row| row
                .get::<_, String>(0))
            .expect("existing row"),
        "private row"
    );
}

#[test]
fn future_and_negative_versions_fail_without_schema_writes() {
    for unsupported in [-1, 99] {
        let mut connection = Connection::open_in_memory().expect("SQLite");

        connection
            .pragma_update(None, "user_version", unsupported)
            .expect("unsupported version");
        apply(&mut connection).expect_err("unsupported schema");
        assert_eq!(version(&connection), unsupported);
        assert!(schema(&connection).is_empty());
    }
}
