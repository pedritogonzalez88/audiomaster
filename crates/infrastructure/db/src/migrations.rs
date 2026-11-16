use rusqlite::Connection;

const MIGRATIONS: &[(i32, &str)] = &[(1, include_str!("migrations/0001_baseline.sql"))];

pub fn run(connection: &mut Connection) -> Result<(), super::DbError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version    INTEGER PRIMARY KEY,
            applied_at INTEGER NOT NULL,
            checksum   TEXT NOT NULL
        );",
    )?;

    for (version, sql) in MIGRATIONS {
        let applied: Option<i64> = connection
            .query_row(
                "SELECT version FROM schema_migrations WHERE version = ?",
                [version],
                |row| row.get(0),
            )
            .ok();

        if applied.is_some() {
            continue;
        }

        let transaction = connection.transaction()?;
        transaction.execute_batch(sql)?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, applied_at, checksum)
             VALUES (?, ?, ?)",
            rusqlite::params![
                version,
                chrono::Utc::now().timestamp_millis(),
                simple_checksum(sql)
            ],
        )?;
        transaction.commit()?;
    }

    Ok(())
}

fn simple_checksum(input: &str) -> String {
    // FNV-1a 64-bit: enough to detect accidental edits without pulling a crypto
    // dependency into the migration runner.
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}
