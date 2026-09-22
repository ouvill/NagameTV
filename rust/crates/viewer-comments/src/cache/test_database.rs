//! Raw connections for corruption, legacy-schema and lock-contention fixtures.
use futures_lite::future::block_on;
use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};
use std::path::Path;

pub fn open(path: impl AsRef<Path>) -> Result<SqliteConnection, sqlx::Error> {
    block_on(SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true),
    ))
}
