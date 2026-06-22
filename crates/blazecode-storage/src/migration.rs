//! # Migration from flat files to SQLite.
//!
//! Handles the migration from the legacy `sessions.json` + `*.jsonl` file format
//! to the new SQLite + WAL storage.

use crate::schema::SCHEMA_VERSION;
use sqlx::SqlitePool;
use tracing::{debug, info};

/// Migration runner for the store.
#[derive(Debug, Clone)]
pub struct Migration {
    pool: SqlitePool,
}

impl Migration {
    /// Create a new migration runner.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Run all pending migrations.
    pub async fn run(&self) -> Result<(), crate::StoreError> {
        let current_version = self.current_version().await?;

        if current_version < SCHEMA_VERSION {
            info!(
                "migration: {} → {}",
                current_version, SCHEMA_VERSION
            );

            // Version 0 → 1: nothing yet, schema is created by init
            if current_version < 1 {
                self.apply_v1().await?;
            }

            // Record the new version
            self.set_version(SCHEMA_VERSION).await?;
            info!("migration: complete at version {}", SCHEMA_VERSION);
        } else {
            debug!("migration: already at version {}", SCHEMA_VERSION);
        }

        Ok(())
    }

    /// Get the current schema version from the database.
    async fn current_version(&self) -> Result<i64, crate::StoreError> {
        let row: Result<(i64,), sqlx::Error> = sqlx::query_as(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        )
        .fetch_one(&self.pool)
        .await;

        match row {
            Ok((v,)) => Ok(v),
            Err(_) => Ok(0), // Table doesn't exist yet
        }
    }

    /// Record a schema version.
    async fn set_version(&self, version: i64) -> Result<(), crate::StoreError> {
        sqlx::query("INSERT INTO schema_version (version) VALUES ($1)")
            .bind(version)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Apply migration to version 1.
    ///
    /// At v1, the schema is fully defined by `ALL_DDL` in the schema module.
    /// This migration is a placeholder for future schema changes.
    async fn apply_v1(&self) -> Result<(), crate::StoreError> {
        info!("migration v1: initial schema already applied");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StoreConfig;

    #[tokio::test]
    async fn test_migration_runs_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let config = StoreConfig::new(dir.path().join("migrate.db"));
        let pool = crate::Store::open(&config).await.unwrap();
        // Migration ran automatically during open
        // Verify schema version was recorded
        let (version,): (i64,) = sqlx::query_as("SELECT COALESCE(MAX(version), 0) FROM schema_version")
            .fetch_one(pool.pool())
            .await
            .unwrap();
        assert!(version >= 1);
    }
}
